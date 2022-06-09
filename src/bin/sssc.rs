use stateful_split_screen::errors::GenericError;
use stateful_split_screen::socket::*;
use stateful_split_screen::commands::*;
use stateful_split_screen::data::*;
use std::os::unix::net::UnixDatagram;
use clap::{AppSettings, App, SubCommand};

fn main() -> Result<(), GenericError> {
    let matches = App::new("Stateful Split Screen Client")
        .setting(AppSettings::ArgRequiredElseHelp)
        .subcommand(SubCommand::with_name(RESTORE)
                    .help("Restores window to original dimensions"))
        .subcommand(SubCommand::with_name(SPLITLEFT)
                    .help("Resize the window to cover the left half of the desktop"))
        .subcommand(SubCommand::with_name(SPLITRIGHT)
                    .help("Resize the window to cover the right half of the desktop"))
        .subcommand(SubCommand::with_name(MAXIMIZE)
                    .help("Maximize the window"))
        .subcommand(SubCommand::with_name(SAVE)
                    .help("Save the current window dimensions"))
        .subcommand(SubCommand::with_name(RESTART)
                    .help("Restart the server"))
        .subcommand(SubCommand::with_name(QUIT)
                    .help("Shutdown the server"))
        .get_matches();
    let commands_strings = [RESTORE, SPLITLEFT, SPLITRIGHT, MAXIMIZE, SAVE, RESTART, QUIT];
    let command = commands_strings.iter().find(|cmd| matches.subcommand_matches(cmd).is_some()).unwrap();
    let socket = match UnixDatagram::unbound() {
        Ok(sock) => sock,
        Err(_) => return Err(GenericError::new("unbound socket creation")),
    };
    let server_path = get_socket_file()?;
    let mut message = Message::new();
    message.insert(COMMAND, command);
    let message_enc = encode_data(message)?;
    if let Err(_) = socket.send_to(&message_enc, server_path.as_path()) {
        return Err(GenericError::new("send message to socket"));
    }

    Ok(())
}
