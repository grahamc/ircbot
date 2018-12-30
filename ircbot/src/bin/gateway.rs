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
use irc::proto::Command::Raw;
use irc::client::prelude::ServerExt;
use irc::client::prelude::IrcServer;

use amqp::protocol::basic::Deliver;
use amqp::protocol::basic::BasicProperties;
use amqp::Basic;
use amqp::Channel;
use amqp::Session;
use amqp::Table;


use ircbot::config;

use std::time;
use std::thread;
use std::env;

#[derive(Serialize, Deserialize, Debug)]
struct MessageToIRC {
    target: String,
    body: String,
    #[serde(default = "default_irc_message_type")]
    message_type: IRCMessageType
}

#[derive(Serialize, Deserialize, Debug)]
struct MessageFromIRC {
    from: String,
    sender: String,
    body: String
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "lowercase")]
enum IRCMessageType {
    Privmsg,
    Notice
}

fn default_irc_message_type() -> IRCMessageType {
    IRCMessageType::Privmsg
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

    let mut session = Session::open_url(&cfg.rabbitmq.as_uri()).unwrap();
    println!("Connected to rabbitmq");
    println!("About to open channel #1");
    let mut writechan = session.open_channel(1).unwrap();

    let writeexch = writechan.exchange_declare(
        "exchange-messages",  // exchange name
        "fanout", //  exchange type
        false, //
        true, //
        false, //
        false,
        false,
        Table::new()
    );


    let mut readchan = session.open_channel(2).unwrap();
    //queue: &str, passive: bool, durable: bool, exclusive: bool, auto_delete: bool, nowait: bool, arguments: Table
    let readqueue = readchan.queue_declare("queue-publish", false, true, false, false, false, Table::new());

    let server = IrcServer::from_config(cfg.irc_config()).unwrap();
    server.identify().unwrap();
    let reader = server.clone();
    let reader_updater = server.clone();

    thread::spawn(move || {
        let consumer_name = readchan.basic_consume(
            move |_channel: &mut Channel, _deliver: Deliver, _headers: BasicProperties, body: Vec<u8>| {
                let msg: Result<MessageToIRC, serde_json::Error> = serde_json::from_slice(&body);
                if let Ok(msg) = msg {
                    match msg.message_type {
                        IRCMessageType::Notice => {
                                server.send_notice(&msg.target, &msg.body).unwrap();
                        }
                        IRCMessageType::Privmsg => {
                                server.send_privmsg(&msg.target, &msg.body).unwrap();
                        }
                    }
                }
            },
            "queue-publish", "", false, true, false, false, Table::new());
        println!("Starting consumer {:?}", consumer_name);
        // server.stream().map(|m| print!("{}", m)).wait().count();

        readchan.start_consuming();
        readchan.close(200, "Bye").unwrap();
        panic!("Lost the consumer!");
    });


    reader.for_each_incoming(|message| {
        match message.command {
            Raw(code, mut elems, extra) => {
                if code == "470" {
                    if let Some(_who) = elems.pop() {
                        if let Some(channel) = elems.pop() {
                            if let Some(target) = elems.pop() {
                                if channel.starts_with("#") {
                                    println!("Forwarded from {:?} to {:?}, retrying",
                                             channel, target
                                    );
                                    thread::sleep(time::Duration::from_secs(5));
                                    reader_updater.send_join(&channel).unwrap();
                                } else {
                                    println!("RAW: {:?}, {:?}, {:?}", code, elems, extra);
                                }
                            } else {
                                println!("RAW: {:?}, {:?}, {:?}", code, elems, extra);
                            }
                        } else {
                            println!("RAW: {:?}, {:?}, {:?}", code, elems, extra);
                        }
                    } else {
                        println!("RAW: {:?}, {:?}, {:?}", code, elems, extra);
                    }
                } else {
                    println!("RAW: {:?}, {:?}, {:?}", code, elems, extra);
                }
            }
            Command::Response(ERR_NOCHANMODES, mut deets, mut msg) => {
                if let Some(_who) = deets.pop() {
                    if let Some(channel) = deets.pop() {
                        if channel.starts_with("#") {
                            println!("Failed to join {:?}, retrying: {:?} ",
                                     channel, msg
                            );
                            thread::sleep(time::Duration::from_secs(5));
                            reader_updater.send_join(&channel).unwrap();
                        } else {
                            println!("no chan modes: {:?}, {:?}", deets, msg);
                        }
                    } else {
                        println!("no chan modes: {:?}, {:?}", deets, msg);
                    }
                } else {
                    println!("no chan modes: {:?}, {:?}", deets, msg);
                }
            }
            Command::PRIVMSG(ref _target, ref msg) => {
                let msg = serde_json::to_string(&MessageFromIRC{
                    from: message.response_target()
                        .expect("a response target for a privmsg")
                        .to_owned(),
                    sender: message.source_nickname()
                        .expect("a source nickname for a privmsg")
                        .to_owned(),
                    body: msg.clone(),
                }).unwrap();

                writechan.basic_publish(
                    "exchange-messages".to_owned(),
                    "".to_owned(),
                    false,
                    false,
                    BasicProperties {
                        content_type: Some("application/json".to_owned()),
                        ..Default::default()
                    },
                    msg.into_bytes()
                ).expect("Failed to publish message");
            }
            _ => {
                print!("{:?}\n", message.command);
            },
        }
    }).unwrap();
}
