extern crate notify;
extern crate rev_lines;

use std::env;

use serde::{Deserialize, Serialize};

use std::sync::mpsc::{channel, Receiver};
use notify::{Watcher, RecommendedWatcher, RecursiveMode, RawEvent, raw_watcher};

use std::fs::{self,File};
use std::io::BufReader;
use rev_lines::RevLines;
use regex::Regex;

use tokio::runtime::Runtime;
use slack_hook2::{Slack, PayloadBuilder};

#[derive(Serialize, Deserialize)]
struct Config {
    name: String,
    url: String,
    path: String,
    hook: String,
}

struct Channel {
    watcher: RecommendedWatcher,
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

        // println macro
        println!("Add:{} to watching list", &config.path);

        Channel {
            receiver: receiver,
            watcher: watcher,
            config: config,
            slack: slack,
            logs: Vec::new(),
        }
    }

    fn read_logs(&mut self) -> &mut Channel
    {
        // clear previous logs
        self.logs = Vec::new();
        self.logs.push(self.config.name.clone());
        self.logs.push(self.config.url.clone());

        // set regexp
        let first_line_re = Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} ").unwrap();
        let reuqest_re = Regex::new(r"^Request URL: ").unwrap();

        // readfile
        let file = File::open(&self.config.path).unwrap();
        let rev_lines = RevLines::new(BufReader::new(file)).unwrap();
        for line in rev_lines
        {
            if reuqest_re.is_match(&line) {
                self.logs.push(line);
                continue;
            }
            if first_line_re.is_match(&line) {
                self.logs.push(line);
                break;
            }
        }

        self
    }

    async fn send_to_slack(&mut self) -> &mut Channel
    {
        let txt: String = self.logs.join("\n");

        let p = PayloadBuilder::new()
        .text(txt)
        .icon_emoji(":chart_with_upwards_trend:")
        .build()
        .unwrap();

        let res = self.slack.send(&p).await;

        match res {
            Ok(()) => { /* silence is golden */ },
            Err(err) => {
                println!("Error: {}", err);
            },
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
                Ok(_event) => {
                    // async time !!!
                    Runtime::new()
                    .expect("Failed to create Tokio runtime")
                    .block_on(channel.read_logs().send_to_slack());
                },
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    }
}
