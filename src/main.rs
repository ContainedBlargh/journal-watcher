#[macro_use]
extern crate nickel;

use argparse::{ArgumentParser, Store};
use nickel::{HttpRouter, Nickel, MediaType, QueryString};
use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::fs;
use std::sync::{Mutex, Arc};
use std::collections::HashMap;
use chrono::*;
use serde::{Serialize, Deserialize};
use std::convert::TryFrom;
use dirs::home_dir;

mod patterns;
use patterns::{EventPattern, RawEventPattern};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    timestamp: u64,
    attributes: HashMap<String, String>
}

fn parse_timestamp_loosely(s: &str) -> Option<u64> {
    if let Ok(i) = s.parse::<u64>() { 
        return Some(i); 
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp() as u64);
    }
    if let Ok(dt) = Local.datetime_from_str(s, "%FT%T") {
        return Some(dt.timestamp() as u64);
    }
    if let Ok(dt) = Local.datetime_from_str(format!("{}T00:00:00", s).as_str(), "%FT%T") {
        return Some(dt.timestamp() as u64);
    }
    return None;
}

fn default_path() -> Option<String> {
    let mut path = home_dir()?;
    path.push(".journal-watcher");
    Some(path.to_str()?.to_string())
}

fn main() {
    let mut port: u16 = 6767;
    let mut patterns_path = String::new();
    let mut unit = String::new();
    let mut db_path: String = default_path().unwrap();
    {
        let mut ap = ArgumentParser::new();
        ap.refer(&mut port)
            .add_option(&["-p", "--port"], Store, "The port to serve data to.");
        ap.refer(&mut db_path).add_option(
            &["-d", "--db-path"], 
            Store, 
            "Path to the location where the database should be stored. Defaults to ~/.journal-watcher/data.sled"
        );
        ap.refer(&mut patterns_path).add_argument(
            "patterns file path",
            Store,
            "The path to the pattern file used to identify notable events.",
        );
        ap.refer(&mut unit)
            .add_argument("unit", Store, "The systemd unit to listen to. Defaults to 6767.");
        ap.parse_args_or_exit();
    }
    if patterns_path.is_empty() {
        panic!("No patterns file selected, use --help for usage details.")
    }
    if unit.is_empty() {
        panic!("No systemd unit selected, use --help for usage details.")
    }

    let db = Arc::new(Mutex::new(sled::open(db_path).unwrap()));
    let db_listen = db.clone();

    let patterns_reader = fs::File::open(patterns_path.clone())
        .and_then(|f|Ok(BufReader::new(f)))
        .unwrap();
    
    let raw_event_patterns: Vec<RawEventPattern> = match patterns_path {
        json if json.ends_with(".json") => serde_json::from_reader(patterns_reader).unwrap(),
        yaml if yaml.ends_with(".yaml") => serde_yaml::from_reader(patterns_reader).unwrap(),
        _ => panic!("Patterns file must be a .json or .yaml file!")
    };
    let event_patterns: Vec<EventPattern> = raw_event_patterns.into_iter().map(|rep| EventPattern::try_from(rep).unwrap()).collect();
    let mut event_patterns_copy = event_patterns.to_vec();
    event_patterns_copy.sort_unstable_by_key(|ep| (ep.event).clone());
    let listen_thread = std::thread::spawn(move || {
        //Read lines from journalctl -f
        let journalctl  = Command::new("journalctl")
            .args(&["-u", &unit])
            .args(&["-o", "cat"])
            .arg("-f")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let reader = BufReader::new(journalctl.stdout.unwrap());
        //Apply patterns to the lines and capture events.
        for line in reader.lines().map(move |l|l.unwrap()) {
            //If the line matches a pattern, save it as an event.
            for EventPattern{event, pattern, attribute_patterns} in (&event_patterns).into_iter() {
                let strline = line.as_str();
                if !pattern.is_match(strline) {
                    continue;
                }
                let timestamp = Local::now().timestamp() as u64;

                let mut attributes: HashMap<String, String> = HashMap::new();
                for (attribute, pattern) in attribute_patterns.into_iter() {
                    if let Some(captures) = pattern.captures(strline) {
                        let found: Vec<_> = (1..captures.len())
                            .into_iter()
                            .map(move |i|(&captures).get(i).unwrap())
                            .map(|cap|cap.as_str().to_string())
                            .collect();
                        let joined = found.join(", ");
                        attributes.insert(attribute.to_string(), joined);
                    }
                }
                let db_shared = db_listen.lock().unwrap();
                let tree = db_shared.open_tree(event).unwrap();
                let event = Event { timestamp, attributes };
                let serialized = serde_cbor::to_vec(&event).unwrap();
                tree.insert(format!("{}", timestamp), serialized).unwrap();
            }
        }
    });

    let db_server = db.clone();
    let _server_thread = std::thread::spawn(move || {
        let mut server = Nickel::new();
        server.get("/", middleware! {|req, mut res|
            
            let start_ts = req.query()
                .get("start")
                .and_then(parse_timestamp_loosely)
                .unwrap_or(0);
            let start = format!("{}", start_ts);
            
            let end_ts = req.query()
                .get("end")
                .and_then(parse_timestamp_loosely)
                .unwrap_or(u64::MAX);
            let end = format!("{}", end_ts);

            let db_shared = db_server.lock().unwrap();
            //Read latest data.
            let mut data_entries: HashMap<String, Vec<Event>> = HashMap::new();
            for EventPattern{event, pattern: _, attribute_patterns: _} in &event_patterns_copy {
                let tree = db_shared.open_tree(event).unwrap();
                let range = tree.range(start.as_str()..end.as_str());
                let mut vector = if let Some(vec) = data_entries.get(event) {
                    vec.to_vec()
                } else {
                    vec!()
                };
                for entry_res in range {
                    let (_, value) = entry_res.unwrap();
                    let event: Event = serde_cbor::from_slice(&value).unwrap();
                    vector.push(event);
                }
                data_entries.insert(event.as_str().to_string(), vector);
            }
            res.set(MediaType::Json);
            serde_json::to_string_pretty(&data_entries).unwrap()
        });
        server.listen("localhost:6767").unwrap();
    });
    
    //Stop when the listen_thread receives EOF.
    listen_thread
        .join()
        .unwrap();
}
