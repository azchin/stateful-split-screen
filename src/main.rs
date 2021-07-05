use stateful_split_screen::xcb::*;
use stateful_split_screen::socket::*;
use stateful_split_screen::errors::GenericError;
use stateful_split_screen::commands::*;
use stateful_split_screen::data::*;
use std::collections::HashMap;

const SOCKET_BUFFER_LEN: usize = 1024;

#[derive(PartialEq)]
enum State {
    Windowed,
    SplitLeft,
    SplitRight,
}

struct Dimensions {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

struct Properties {
    state: State,
    dimensions: Dimensions,
}

fn do_single_command(connections: &XCBConnections,
                     window_properties: &mut HashMap<xcb::xproto::Window, Properties>,
                     message: Message)
                     -> Result<(), GenericError>
{
    let base = &connections.base;
    let ewmh = &connections.ewmh;
    let screen = connections.screen;
    let (work_x, work_y, work_width, work_height) = get_work_area(ewmh, screen)?;
    let (work_x, work_y, work_width, work_height) = (work_x as i16,
                                                     work_y as i16,
                                                     work_width as u16,
                                                     work_height as u16);
    let half_width = work_width / 2;
    let active_window = get_active_window(ewmh, screen)?;
    let (window_x, window_y, window_width, window_height) = get_geometry(base, ewmh, active_window)?;

    #[cfg(feature = "debug")]
    println!("{} {} {} {} {} {}", active_window, message, window_x, window_y, window_width, window_height);

    let window_prop = window_properties.get(&active_window);
    if window_prop.is_none() || window_prop.unwrap().state == State::Windowed {
        let dim = Dimensions{x: window_x, y: window_y, width: window_width, height: window_height};
        let prop = Properties{state: State::Windowed, dimensions:dim};
        window_properties.insert(active_window, prop);
    }

    match message.command().as_str() {
        RESTORE => {
            match window_properties.get_mut(&active_window) {
                Some(prop) if prop.state == State::Windowed => return Ok(()),
                Some(prop) => {
                    prop.state = State::Windowed;
                    let dim = &prop.dimensions;
                    move_resize(base, ewmh, active_window, dim.x, dim.y, dim.width, dim.height)?;
                },
                None => return Err(GenericError::new("cannot find active window in memory")),
            };
        },
        SPLITLEFT => {
            match window_properties.get_mut(&active_window) {
                Some(prop) if prop.state == State::SplitLeft => return Ok(()),
                Some(prop) => prop.state = State::SplitLeft,
                None => return Err(GenericError::new("cannot find active window in memory")),
            }
            move_resize(base, ewmh, active_window, work_x, work_y, half_width, work_height)?;
        },
        SPLITRIGHT => {
            match window_properties.get_mut(&active_window) {
                Some(prop) if prop.state == State::SplitRight => return Ok(()),
                Some(prop) => prop.state = State::SplitRight,
                None => return Err(GenericError::new("cannot find active window in memory")),
            }
            move_resize(base, ewmh, active_window, half_width as i16, work_y, half_width, work_height)?;
        },
        _ => return Err(GenericError::new("invalid command")),
    }
    Ok(())
}

fn main() -> Result<(), GenericError> {
    let mut window_properties: HashMap<xcb::xproto::Window, Properties> = HashMap::new();
    let mut connections = setup_connections()?;

    let socket = bind_socket()?;

    loop {
        let mut buf = vec![0; SOCKET_BUFFER_LEN];
        let (size, _sender) = match socket.recv_from(&mut buf) {
            Ok((sz, sndr)) => (sz, sndr),
            Err(e) => {
                eprintln!("{}", e);
                continue;
            },
        };
        let message = decode_data(&buf[0..size])?;

        match message.command().as_str() {
            RESTART => {
                connections = setup_connections()?;
                continue;
            },
            _ => (),
        }

        if let Err(e) = do_single_command(&connections,
                                          &mut window_properties,
                                          message)
        {
            eprintln!("{}", e);
        }
    }

    // remove_socket_file()?;
    // Ok(())
}
