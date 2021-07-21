use std::convert::TryFrom;
use regex::Regex;
use serde::{Deserialize};
use std::collections::HashMap;

#[derive(Deserialize, Debug, Clone)]
pub struct RawEventPattern {
    pub event: String,
    pub pattern: String,
    pub attribute_patterns: HashMap<String, String>
}

#[derive(Debug, Clone)]
pub struct EventPattern {
    pub event: String,
    pub pattern: Regex,
    pub attribute_patterns: HashMap<String, Regex>
}

impl TryFrom<RawEventPattern> for EventPattern {
    type Error = String;
    fn try_from(rep: RawEventPattern) -> Result<Self, Self::Error> {
        let event = rep.event;
        let mut parsed_patterns: HashMap<String, Regex> = HashMap::new();
        if let Ok(re) = Regex::new(rep.pattern.as_str()) {
            for (k, v) in rep.attribute_patterns.into_iter() {
                match Regex::new(&v) {
                    Ok(re) => { parsed_patterns.insert(k, re); },
                    _ => return Err(format!("Could not parse regex '{}' for attribute '{}' in event '{}'.", v, k, event))
                }
            }
            return Ok(EventPattern{ event, pattern: re, attribute_patterns: parsed_patterns })
        } else {
            return Err(format!("Could not parse top-level regex '{}' for event '{}'.", rep.pattern, event))
        }
    }
}

