#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate ircbot;
extern crate irc;
extern crate amqp;
extern crate env_logger;

#[macro_use]
extern crate log;
use irc::client::server::Server;
use irc::client::prelude::Command;
use irc::client::prelude::ServerExt;
use irc::client::prelude::IrcServer;
use irc::proto::mode::Mode;
use irc::proto::mode::ChannelMode;
use irc::proto::response::Response;
use std::collections::HashSet;
use ircbot::config;
use std::{thread, time};
use std::env;

#[derive(Serialize, Deserialize, Debug)]
struct Message {
    target: String,
    body: String
}

fn main() {
    if let Err(_) = env::var("RUST_LOG") {
        env::set_var("RUST_LOG", "info");
        env_logger::init().unwrap();
        info!("Defaulting RUST_LOG environment variable to info");
    } else {
        env_logger::init().unwrap();
    }

    let cfg = config::load(env::args().nth(1).unwrap().as_ref());


    let server = IrcServer::from_config(cfg.irc_config()).unwrap();
    server.identify().unwrap();
    let reader = server.clone();

    let mut channels: HashSet<String> = HashSet::new();
    for chan in cfg.channels.iter() {
        channels.insert(chan.clone());
    }

    let mut banned: HashSet<String> = HashSet::new();

    // nickname, channel
    // let mut banqueue: Vec<(String, String)> = vec![];

    reader.for_each_incoming(|message| {
        match message.command {
            Command::PRIVMSG(ref _target, ref msg) => {
                let badwords: Vec<&str> = vec![
                    "▄",
                    "companieshouse.gov.uk",
                    "christel sold",
                    "freenode to private",
                    "snoonet",
                    "nigger",
                    "chan freenode",
                    "supernets.org",
                    "flooding challenge",
                    "details!!",
                    "supernets.org",
                    "irc investigative journalists",
                    "freenode pedophilia",
                    "freenodegate",
                    "bryanostergaard",
                    "mattstrout",
                    "blog by freenode staff member",
                    "http://web.nba1001.net:8888/tj/tongji.js",
                    "nba1001",
                    ":8888",
                    "tongi.js",
                    "tongi",
                    "nenolod",
                    "irc advertising",
                    "sp9002_@efnet",
                    "irc ad service",
                    "entrepreneurs and fentanyl addicts",
                    "williampitcock",
                ];

                if let Some(from) = message.source_nickname() {
                    if from == "gchristensen" {
                        println!("{}", from);
                        if message.response_target() == Some("gchristensen") {
                            println!("Joining {}", msg);
                            server.send_join(msg).unwrap();
                        }
                    }
                    if let Some(_inchan) = message.response_target() {
                        let mut doban: bool = false;
                        for word in badwords {
                            if msg.to_lowercase().contains(word) {
                                doban = true;
                            }
                        }

                        if doban && ! banned.contains(from) {
                            banned.insert(from.to_string());
                            println!("Muting {:?}, {:?}", from, msg);
                            for chan in channels.iter() {
                                server.send_mode(
                                    &chan,
                                    &[
                                        // Technically ::Founder is `q`
                                        // but Freenode's +q is mute
                                        Mode::Plus(
                                            ChannelMode::Unknown('q'),
                                            Some(from.to_owned())
                                        )
                                    ]
                                ).expect("failed to ban");
                                println!("done: {:?}", chan);
                                thread::sleep(time::Duration::from_millis(200));
                            }
                        }
                    }
                }
            }
            Command::Response(Response::ERR_CHANOPRIVSNEEDED, desc, _) => {
                if let Some(channel) = desc.get(2) {
                    println!("Dropping {} from channel list, got CHANOPSNEEDED", channel);
                    channels.remove(channel);
                } else {
                    println!("WAT? {:?}", desc);
                }
            }
            Command::JOIN(ref channel, _, _) => {
                if message.source_nickname() == Some("the") {
                    println!("Attempting ops in {} now that I'm joined", channel);
                    server.send_privmsg("Chanserv",
                                        &format!("op {}", channel)
                    ).unwrap();
                }
            }
            Command::ChannelMODE(ref channel, ref modes) => {
                for mode in modes {
                    match mode {
                        &Mode::Plus(irc::proto::ChannelMode::Oper, Some(ref username)) => {
                            if username == "the" {
                                println!("Got ops in {}", channel);
                                channels.insert(channel.clone());
                            }
                        }
                        _ => {
                            println!("Ignored channel mode change in {}: {:?}",
                                    channel, mode
                            );
                        }
                    }
                }
            }
            _ => {
                print!("{:?}\n", message.command);
            },
        }
    }).unwrap();
}
