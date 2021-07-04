use stateful_split_screen::xcb::*;
use stateful_split_screen::socket::*;
use stateful_split_screen::errors::GenericError;
use stateful_split_screen::commands::*;
use stateful_split_screen::data::*;
use std::collections::HashMap;
use std::os::unix::net::UnixDatagram;

const SOCKET_BUFFER_LEN: usize = 1024;

#[derive(PartialEq)]
enum State {
    Windowed,
    SplitLeft,
    SplitRight,
}

struct Dimensions {
    state: State,
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

fn do_single_command(base: &xcb::base::Connection,
                     ewmh: &xcb_util::ewmh::Connection,
                     screen: i32,
                     work_x: i16,
                     work_y: i16,
                     work_width: u16,
                     work_height: u16,
                     window_dimensions: &mut HashMap<xcb::xproto::Window, Dimensions>,
                     socket: &UnixDatagram)
                     -> Result<(), GenericError>
{
    let mut buf = vec![0; SOCKET_BUFFER_LEN];
    let (size, _sender) = match socket.recv_from(&mut buf) {
        Ok((sz, sndr)) => (sz, sndr),
        Err(_) => return Err(GenericError::new("socket receive")),
    };

    let message = decode_data(&buf[0..size])?;

    let half_width = work_width / 2;
    let active_window = get_active_window(ewmh, screen)?;
    let (window_x, window_y, window_width, window_height) = get_geometry(base, ewmh, active_window)?;

    #[cfg(feature = "debug")]
    println!("{} {} {} {} {} {}", active_window, message, window_x, window_y, window_width, window_height);

    match message {
        RESTORE => {
            let dim = match window_dimensions.get(&active_window) {
                Some(dim) if dim.state != State::Windowed => {
                    move_resize(&base, &ewmh, active_window, dim.x, dim.y, dim.width, dim.height)?;
                    dim
                },
                Some(dim) => dim,
                None => return Err(GenericError::new("cannot find stored window")),
            };
            let dim_new = Dimensions{state: State::Windowed, x: dim.x, y: dim.y, width: dim.width, height: dim.height};
            window_dimensions.insert(active_window, dim_new);
            
        },
        SPLITLEFT => {
            match window_dimensions.get(&active_window) {
                Some(dim) if dim.state == State::SplitLeft => {
                    return Err(GenericError::new("window is already split"));
                },
                Some(dim) if dim.state == State::Windowed => {
                    let dim_new = Dimensions{state: State::SplitLeft, x: window_x, y: window_y, width: window_width, height: window_height};
                    window_dimensions.insert(active_window, dim_new);
                },
                None => { // Feels bad can't join this to the above case
                    let dim_new = Dimensions{state: State::SplitLeft, x: window_x, y: window_y, width: window_width, height: window_height};
                    window_dimensions.insert(active_window, dim_new);
                },
                _ => (),
            }
            move_resize(&base, &ewmh, active_window, work_x, work_y, half_width, work_height)?;
        },
        SPLITRIGHT => {
            match window_dimensions.get(&active_window) {
                Some(dim) if dim.state == State::SplitRight => {
                    return Err(GenericError::new("window is already split"));
                },
                Some(dim) if dim.state == State::Windowed => {
                    let dim_new = Dimensions{state: State::SplitRight, x: window_x, y: window_y, width: window_width, height: window_height};
                    window_dimensions.insert(active_window, dim_new);
                },
                None => { // Feels bad can't join this to the above case
                    let dim_new = Dimensions{state: State::SplitRight, x: window_x, y: window_y, width: window_width, height: window_height};
                    window_dimensions.insert(active_window, dim_new);
                },
                _ => (),
            }
            move_resize(&base, &ewmh, active_window, half_width as i16, work_y, half_width, work_height)?;
        },
        _ => return Err(GenericError::new("invalid command")),
    }
    Ok(())
}

fn main_with_results() -> Result<(), GenericError> {
    let mut window_dimensions: HashMap<xcb::xproto::Window, Dimensions> = HashMap::new();
    let (base_connection, ewmh_connection, default_screen) = setup_connections()?;
    let (work_x, work_y, work_width, work_height) = get_work_area(&ewmh_connection, default_screen)?;

    let socket = bind_socket()?;

    loop {
        if let Err(e) = do_single_command(&base_connection,
                                          &ewmh_connection,
                                          default_screen,
                                          work_x as i16,
                                          work_y as i16,
                                          work_width as u16,
                                          work_height as u16,
                                          &mut window_dimensions,
                                          &socket)
        {
            eprintln!("{}", e);
        }
    }

    // remove_socket_file()?;
    // Ok(())
}

fn main() {
    if let Err(e) = main_with_results() {
        eprintln!("{}", e);
    }
}
