//! What PipeWire's registry says about audio devices (parity matrix NEW-B, AUD-1): the nodes that
//! are microphones or outputs, their names, and which ones are the defaults.
//!
//! It reads the registry the way a user would, through the tools PipeWire ships (`pw-dump` for the
//! whole graph as JSON, `pw-metadata` for one default): no extra library, and the parsing is plain
//! text handling that is tested on every OS with captured output. A system without those tools
//! answers an error and the caller falls back to the one "default" entry. The recorder polls
//! [`default_name`] a few times a minute (a short process), never `pw-dump`.

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use serde_json::Value;
use std::process::Stdio;

/// One audio node of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PwNode {
    pub id: u64,
    /// `node.name`: stable, what `target.object` takes.
    pub name: String,
    /// `node.description`: what a person sees.
    pub description: String,
    /// `media.class`, e.g. `Audio/Source`.
    pub class: String,
    /// `audio.channels` when the node says it, else 0.
    pub channels: u16,
}

/// The audio part of the registry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    pub nodes: Vec<PwNode>,
    /// `node.name` of `default.audio.source` in the `default` metadata.
    pub default_source: Option<String>,
    /// `node.name` of `default.audio.sink`.
    pub default_sink: Option<String>,
}

impl Registry {
    /// Microphones: real and virtual sources (not video sources, not sink monitors).
    pub fn sources(&self) -> impl Iterator<Item = &PwNode> {
        self.nodes
            .iter()
            .filter(|n| n.class == "Audio/Source" || n.class == "Audio/Source/Virtual")
    }

    /// Output devices (sinks); their monitors are what "system audio" records.
    #[allow(dead_code)] // the output picker (system-audio device choice) will use it
    pub fn sinks(&self) -> impl Iterator<Item = &PwNode> {
        self.nodes
            .iter()
            .filter(|n| n.class == "Audio/Sink" || n.class == "Audio/Sink/Virtual")
    }
}

/// Parses the JSON `pw-dump` prints.
pub fn parse_pw_dump(json: &str) -> Result<Registry, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("pw-dump output: {e}"))?;
    let objects = v.as_array().ok_or("pw-dump output is not a list")?;
    let mut reg = Registry::default();
    for o in objects {
        let ty = o.get("type").and_then(Value::as_str).unwrap_or_default();
        if ty == "PipeWire:Interface:Node" {
            let Some(props) = o.pointer("/info/props") else {
                continue;
            };
            let class = props
                .get("media.class")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !class.starts_with("Audio/") {
                continue;
            }
            let Some(name) = props.get("node.name").and_then(Value::as_str) else {
                continue;
            };
            let description = props
                .get("node.description")
                .or_else(|| props.get("node.nick"))
                .and_then(Value::as_str)
                .unwrap_or(name);
            reg.nodes.push(PwNode {
                id: o.get("id").and_then(Value::as_u64).unwrap_or(0),
                name: name.to_string(),
                description: description.to_string(),
                class: class.to_string(),
                channels: props
                    .get("audio.channels")
                    .and_then(Value::as_u64)
                    .and_then(|c| u16::try_from(c).ok())
                    .unwrap_or(0),
            });
        } else if ty == "PipeWire:Interface:Metadata"
            && o.pointer("/props/metadata.name").and_then(Value::as_str) == Some("default")
        {
            for m in o
                .get("metadata")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let key = m.get("key").and_then(Value::as_str).unwrap_or_default();
                let name = m
                    .pointer("/value/name")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                match key {
                    "default.audio.source" => reg.default_source = name,
                    "default.audio.sink" => reg.default_sink = name,
                    _ => {}
                }
            }
        }
    }
    Ok(reg)
}

/// Parses `pw-metadata -n default 0 <key>` output: a line like
/// `update: id:0 key:'default.audio.source' value:'{"name":"alsa_input..."}' type:'Spa:String:JSON'`.
/// A removed default prints no `update` line.
pub fn parse_metadata_default(output: &str, key: &str) -> Option<String> {
    let marker = format!("key:'{key}' value:'");
    let line = output.lines().find(|l| l.contains(&marker))?;
    let rest = &line[line.find(&marker)? + marker.len()..];
    let value = &rest[..rest.find("' type:").unwrap_or(rest.len())];
    let v: Value = serde_json::from_str(value).ok()?;
    v.get("name").and_then(Value::as_str).map(str::to_string)
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = cm_platform::process::command(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {program}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{program} exited with {}", out.status));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Reads the registry now (one `pw-dump` run, about 50 ms).
pub fn query() -> Result<Registry, String> {
    parse_pw_dump(&run("pw-dump", &[])?)
}

/// The current default for `key` (`default.audio.source` or `default.audio.sink`), one short
/// `pw-metadata` run; `None` when the tool is missing or no default is set.
pub fn default_name(key: &str) -> Option<String> {
    parse_metadata_default(&run("pw-metadata", &["-n", "default", "0", key]).ok()?, key)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Trimmed `pw-dump` of the lab laptop (Ubuntu 24.04, PipeWire 1.0.5) plus a USB headset.
    const DUMP: &str = r#"[
      {"id":0,"type":"PipeWire:Interface:Core","info":{"props":{}}},
      {"id":36,"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},
       "metadata":[
         {"subject":0,"key":"default.video.source","type":"Spa:String:JSON","value":{"name":"v4l2_input.cam"}},
         {"subject":0,"key":"default.audio.sink","type":"Spa:String:JSON","value":{"name":"alsa_output.analog-stereo"}},
         {"subject":0,"key":"default.audio.source","type":"Spa:String:JSON","value":{"name":"alsa_input.usb-headset"}}]},
      {"id":31,"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"settings"},
       "metadata":[{"subject":0,"key":"default.audio.source","value":{"name":"not-this-one"}}]},
      {"id":45,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Video/Source","node.name":"v4l2_input.cam","node.description":"Integrated Camera"}}},
      {"id":51,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Sink","node.name":"alsa_output.analog-stereo","node.description":"Built-in Audio Analog Stereo","audio.channels":2}}},
      {"id":52,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Source","node.name":"alsa_input.pci.analog-stereo","node.description":"Built-in Audio Analog Stereo","audio.channels":2}}},
      {"id":70,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Audio/Source","node.name":"alsa_input.usb-headset","node.nick":"USB Headset","audio.channels":1}}},
      {"id":71,"type":"PipeWire:Interface:Node","info":{"props":{"media.class":"Stream/Output/Audio","node.name":"firefox"}}}
    ]"#;

    #[test]
    fn microphones_sinks_and_defaults_come_from_the_dump() {
        let r = parse_pw_dump(DUMP).unwrap();
        let mics: Vec<_> = r.sources().map(|n| n.name.as_str()).collect();
        assert_eq!(
            mics,
            ["alsa_input.pci.analog-stereo", "alsa_input.usb-headset"]
        );
        assert_eq!(r.sinks().count(), 1);
        assert_eq!(r.default_source.as_deref(), Some("alsa_input.usb-headset"));
        assert_eq!(r.default_sink.as_deref(), Some("alsa_output.analog-stereo"));
    }

    #[test]
    fn a_node_without_a_description_uses_its_nick_then_its_name() {
        let r = parse_pw_dump(DUMP).unwrap();
        let usb = r.sources().find(|n| n.id == 70).unwrap();
        assert_eq!(usb.description, "USB Headset");
        assert_eq!(usb.channels, 1);
    }

    /// AUD-1 on a machine with PipeWire: the real registry lists a source and names the default
    /// (run with `--ignored --nocapture`; opens no stream).
    #[test]
    #[ignore = "needs a running PipeWire"]
    fn the_real_registry_lists_sources_and_a_default() {
        let r = query().unwrap();
        for n in r.sources() {
            println!("source {} {:?} ch={}", n.name, n.description, n.channels);
        }
        for n in r.sinks() {
            println!("sink {} {:?}", n.name, n.description);
        }
        println!("defaults: {:?} / {:?}", r.default_source, r.default_sink);
        assert!(r.sources().count() >= 1);
        assert_eq!(default_name("default.audio.source"), r.default_source);
        assert_eq!(default_name("default.audio.sink"), r.default_sink);
    }

    #[test]
    fn an_unreadable_dump_is_an_error() {
        assert!(parse_pw_dump("not json").is_err());
        assert!(parse_pw_dump("{}").is_err());
        assert_eq!(parse_pw_dump("[]").unwrap(), Registry::default());
    }

    #[test]
    fn the_metadata_tool_output_names_the_default() {
        let out = "Found \"default\" metadata 36\nupdate: id:0 key:'default.audio.source' value:'{\"name\":\"alsa_input.x\"}' type:'Spa:String:JSON'\n";
        assert_eq!(
            parse_metadata_default(out, "default.audio.source").as_deref(),
            Some("alsa_input.x")
        );
        assert_eq!(parse_metadata_default(out, "default.audio.sink"), None);
        assert_eq!(
            parse_metadata_default("Found \"default\" metadata 36\n", "default.audio.source"),
            None
        );
    }
}
