# journal-watcher
An HTTP-API that allows reading and searching through `journalctl`.

## Description

An executable written in Rust that exposes a single HTTP endpoint with JSON-data based on events written to the `systemd` log for a particular unit. The events are recognized by a set of regular expressions that have to be declared in a JSON-file (the patterns file). The expressions are organized as one top-level expression that recognizes an event and several attribute expressions that try to retrieve relevant data from the log-entry (single lines only) using capture groups.

When the service starts, the patterns are loaded and parsed.
Then, a thread launches `journalctl` as a subprocess and follows the output. Whenever an event-pattern is matched, the output is stored in a [`sled`-database](https://sled.rs/), which allows for moving back and forth in the event history. 
Simultaneously, another thread launches and exposes the event history as a single JSON-object.

I use this for monitoring hobby projects where I feel that setting up a full-scale time-series database is too cumbersome.

## Usage

```
Usage:
  journal-watcher [OPTIONS] [PATTERNS FILE PATH] [UNIT]


Positional arguments:
  patterns file path    The path to the pattern file used to identify notable
                        events.
  unit                  The systemd unit to listen to. Defaults to 6767.

Optional arguments:
  -h,--help             Show this help message and exit
  -p,--port PORT        The port to serve data to.
  -d,--db-path DB_PATH  Path to the location where the database should be
                        stored. Defaults to ~/.journal-watcher/data.sled
```


