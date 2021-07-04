use stateful_split_screen::errors::GenericError;
use stateful_split_screen::socket::*;
use stateful_split_screen::commands::*;
use stateful_split_screen::data::*;
use std::os::unix::net::UnixDatagram;
use clap::{Arg, App};

fn main() -> Result<(), GenericError> {
    let matches = App::new("Stateful Split Screen Client")
        .arg(Arg::with_name("command")
             .takes_value(true)
             .required(true)
             .possible_values(&[RESTORE, SPLITLEFT, SPLITRIGHT])
        ).get_matches();
    let command = matches.value_of("command").unwrap();
    let socket = match UnixDatagram::unbound() {
        Ok(sock) => sock,
        Err(_) => return Err(GenericError::new("unbound socket creation")),
    };
    let server_path = get_socket_file()?;
    let message = encode_data(command)?;
    if let Err(_) = socket.send_to(&message, server_path.as_path()) {
        return Err(GenericError::new("send message to socket"));
    }

    Ok(())
}
