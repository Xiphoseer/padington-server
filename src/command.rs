use displaydoc::Display;
use std::str::FromStr;

#[derive(Display)]
pub enum ParseCommandError {
    /// The command expected an argument (e.g. `{0}|foo`)
    MissingArg(CommandKind),
    /// The command is not known
    UnknownCommand(String),
}

#[derive(Display)]
pub enum CommandKind {
    /// init
    Init,
    /// chat
    Chat,
}

#[derive(Debug, Clone)]
pub enum Command {
    Chat(String),
    Init,
    Close,
}

impl FromStr for CommandKind {
    type Err = ParseCommandError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "init" => Ok(Self::Init),
            "chat" => Ok(Self::Chat),
            _ => Err(ParseCommandError::UnknownCommand(s.to_owned())),
        }
    }
}

impl FromStr for Command {
    type Err = ParseCommandError;
    fn from_str(input: &str) -> Result<Command, ParseCommandError> {
        let (cmd, arg) = if let Some(cmd_len) = input.find('|') {
            let (cmd, r) = input.split_at(cmd_len);
            let (_, arg) = r.split_at(1);
            (cmd, Some(arg))
        } else {
            (input, None)
        };

        match cmd.parse()? {
            CommandKind::Init => Ok(Command::Init),
            CommandKind::Chat => {
                let text = arg.ok_or(ParseCommandError::MissingArg(CommandKind::Chat))?;
                Ok(Command::Chat(text.to_owned()))
            }
        }
    }
}
