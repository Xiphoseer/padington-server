//! # Padington commands

use displaydoc::Display;
use std::str::FromStr;

/// Error when parsing a command
#[derive(Display)]
pub enum ParseCommandError {
    /// The command expected an argument (e.g. `{0}|foo`)
    MissingArg(CommandKind),
    /// The command `{0}` is not known
    UnknownCommand(String),
}

/// A kind of incoming command
#[derive(Display)]
pub enum CommandKind {
    /// init
    Init,
    /// chat
    Chat,
    /// steps
    Steps,
    /// update
    Update,
    /// webrtc
    WebRTC,
}

/// An incoming command
#[derive(Debug, Clone)]
pub enum Command {
    /// A chat message
    Chat(String),
    /// Steps from the server
    Steps(usize, String),
    /// A renamed user
    Update(String),
    /// Initialize with an intended name
    Init(Option<String>),
    /// Close the connection
    Close,
    /// A WebRTC signal for a client
    WebRTC(u64, String),
}

impl FromStr for CommandKind {
    type Err = ParseCommandError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "init" => Ok(Self::Init),
            "chat" => Ok(Self::Chat),
            "steps" => Ok(Self::Steps),
            "update" => Ok(Self::Update),
            "webrtc" => Ok(Self::WebRTC),
            _ => Err(ParseCommandError::UnknownCommand(s.to_owned())),
        }
    }
}

fn split_arg(input: &str) -> (&str, Option<&str>) {
    if let Some(cmd_len) = input.find('|') {
        let (cmd, r) = input.split_at(cmd_len);
        let (_, arg) = r.split_at(1);
        (cmd, Some(arg))
    } else {
        (input, None)
    }
}

impl FromStr for Command {
    type Err = ParseCommandError;
    fn from_str(input: &str) -> Result<Command, ParseCommandError> {
        let (cmd, arg) = split_arg(input);

        match cmd.parse()? {
            CommandKind::Init => Ok(Command::Init(arg.map(str::to_owned))),
            CommandKind::Chat => {
                let text = arg.ok_or(ParseCommandError::MissingArg(CommandKind::Chat))?;
                Ok(Command::Chat(text.to_owned()))
            }
            CommandKind::Update => {
                let text = arg.ok_or(ParseCommandError::MissingArg(CommandKind::Update))?;
                Ok(Command::Update(text.to_owned()))
            }
            CommandKind::WebRTC => {
                let text = arg.ok_or(ParseCommandError::MissingArg(CommandKind::WebRTC))?;
                let (reciever_str, opt_payload) = split_arg(text);
                let payload =
                    opt_payload.ok_or(ParseCommandError::MissingArg(CommandKind::WebRTC))?;
                let reciever: u64 = reciever_str
                    .parse()
                    .map_err(|_| ParseCommandError::MissingArg(CommandKind::WebRTC))?;
                Ok(Command::WebRTC(reciever, payload.to_owned()))
            }
            CommandKind::Steps => {
                let text = arg.ok_or(ParseCommandError::MissingArg(CommandKind::Steps))?;
                let (version_str, opt_steps) = split_arg(text);
                let steps = opt_steps.ok_or(ParseCommandError::MissingArg(CommandKind::Steps))?;
                let version: usize = version_str
                    .parse()
                    .map_err(|_| ParseCommandError::MissingArg(CommandKind::Steps))?;
                Ok(Command::Steps(version, steps.to_owned()))
            }
        }
    }
}
