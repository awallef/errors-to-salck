extern crate notify;
extern crate rev_lines;

use std::env;

use serde::{Deserialize, Serialize};
//use serde_json::Result;

use std::sync::mpsc::{channel, Receiver};
use notify::{Watcher, FsEventWatcher, RecursiveMode, RawEvent, raw_watcher};
//use std::time::Duration;

use std::fs::{self,File};
use std::io::BufReader;
use rev_lines::RevLines;
use regex::Regex;

use slack_hook2::{Slack, PayloadBuilder};

#[derive(Serialize, Deserialize)]
struct Config {
    name: String,
    path: String,
    hook: String,
}

struct Channel {
    watcher: FsEventWatcher,
    receiver: Receiver<RawEvent>,
    slack: Slack,
    config: Config,
    logs: Vec<String>,
}

impl Channel {

    fn new(path: String) -> Channel
    {
        // read JSON
        let contents = fs::read_to_string(path)
        .expect("Something went wrong reading the file");
        let config: Config = serde_json::from_str(&contents).unwrap();

        // add watcher for Config
        let (sender, receiver) = channel();
        let mut watcher = raw_watcher(sender).unwrap();
        watcher.watch(&config.path, RecursiveMode::Recursive).unwrap();

        // add Slack hook
        let slack = Slack::new(&config.hook).unwrap();

        Channel {
            receiver: receiver,
            watcher: watcher,
            config: config,
            slack: slack,
            logs: Vec::new(),
        }
    }

    fn readLogs(&mut self) -> &mut Channel
    {
        let firstLineRe = Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} ").unwrap();
        let reuqestRe = Regex::new(r"^Request URL: ").unwrap();
        let refererRe = Regex::new(r"^Referer URL: ").unwrap();

        let file = File::open(&self.config.path).unwrap();
        let rev_lines = RevLines::new(BufReader::new(file)).unwrap();
        for line in rev_lines
        {
            if reuqestRe.is_match(&line) {
                self.logs.push(line);
                continue;
            }
            if refererRe.is_match(&line) {
                self.logs.push(line);
                continue;
            }
            if firstLineRe.is_match(&line) {
                self.logs.push(line);
                break;
            }
        }

        self
    }

    async fn sendToSlack(&mut self) -> &mut Channel
    {
        let txt: String = self.logs.join("\n");

        let p = PayloadBuilder::new()
        .text(txt)
        .icon_emoji(":chart_with_upwards_trend:")
        .build()
        .unwrap();

        let res = self.slack.send(&p).await;

        match res {
            Ok(()) => println!("ok"),
            Err(error) => println!("ERR: {:?}",error)
        }

        self
    }
}

fn main()
{
    let mut channels:Vec<Channel> = Vec::new();

    // parse folder
    let paths = fs::read_dir("/data01/errors-to-slack").unwrap();
    for path in paths
    {
        let p: String = path.unwrap().path().to_str().unwrap().to_string();
        channels.push(Channel::new(p));
    }

    loop
    {
        for channel in &mut channels
        {

            match channel.receiver.recv() {
                Ok(RawEvent{path: Some(path), op: Ok(op), cookie}) => {
                    channel
                    .readLogs()
                    .sendToSlack();
                },
                Ok(event) => println!("broken event: {:?}", event),
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    }
}
