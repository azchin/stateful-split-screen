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
    Maximized,
}

#[derive(PartialEq, Clone)]
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

fn conditionally_store_dimensions(
    active_window: xcb::xproto::Window,
    window_properties: &mut HashMap<xcb::xproto::Window, Properties>,
    current_dimensions: Dimensions,
    correct_dimensions: Dimensions,
    state: State
) {
    if window_properties.get(&active_window).is_some()
        && window_properties.get(&active_window).unwrap().state == state
        && current_dimensions != correct_dimensions
    {
        let prop = Properties{state: State::Windowed, dimensions: current_dimensions};
        window_properties.insert(active_window, prop);
    }
}

fn do_single_command(
    connections: &XCBConnections,
    window_properties: &mut HashMap<xcb::xproto::Window, Properties>,
    message: Message,
) -> Result<(), GenericError> {
    if let None = message.get(COMMAND) {
        return Err(GenericError::new("command not found in message"));
    }
    let base = &connections.base;
    let ewmh = &connections.ewmh;
    let _default_screen = connections.screen;
    let (active_window, screen) = get_active_window(base, ewmh)?;
    let (window_x, window_y, window_width, window_height) = get_geometry(base, ewmh, active_window)?;
    let (work_x, work_y, work_width, work_height) = get_work_area(ewmh, screen)?;
    let half_width = work_width / 2;

    #[cfg(feature = "debug")]
    println!("id: {}, cmd: {}, x: {}, y: {}, width: {}, height: {}",
             active_window, message.get(COMMAND).unwrap(), window_x, window_y, window_width, window_height);

    // Checks the current state of the window and stores dimensions if necessary
    let is_windowed_state = ( window_properties.get(&active_window).is_none()
                              || window_properties.get(&active_window).unwrap().state == State::Windowed )
        && message.get(COMMAND).unwrap() != SAVE;
    if is_windowed_state {
        let dim = Dimensions{x: window_x, y: window_y, width: window_width, height: window_height};
        let prop = Properties{state: State::Windowed, dimensions:dim};
        window_properties.insert(active_window, prop);
    }
    // Checks for manual resizes on a managed split window
    let current_dimensions = Dimensions {
        x: window_x,
        y: window_y,
        width: window_width,
        height: window_height,
    };
    let splitleft_dimensions = Dimensions {
        x: work_x,
        y: work_y,
        width: half_width, 
        height: work_height,
    };
    let splitright_dimensions = Dimensions {
        x: half_width as i16,
        y: work_y,
        width: half_width, 
        height: work_height,
    };
    let _maximized_dimensions = Dimensions {
        x: work_x,
        y: work_y,
        width: work_width, 
        height: work_height,
    };
    // TODO fix work area dimensions for extended monitor setup
    conditionally_store_dimensions(active_window, window_properties, current_dimensions.clone(), splitleft_dimensions, State::SplitLeft);
    conditionally_store_dimensions(active_window, window_properties, current_dimensions.clone(), splitright_dimensions, State::SplitRight);
    // conditionally_store_dimensions(active_window, window_properties, current_dimensions.clone(), maximized_dimensions, State::Maximized);

    // Process the command and alter the cached window state
    match message.get(COMMAND).unwrap() {
        RESTORE => {
            ewmh_restore(ewmh, active_window, screen)?;
            match window_properties.get_mut(&active_window) {
                Some(prop) => {
                    prop.state = State::Windowed;
                    let dim = &prop.dimensions;
                    move_resize(base, ewmh, active_window, dim.x, dim.y, dim.width, dim.height)?;
                },
                None => return Err(GenericError::new("cannot find active window in memory")),
            };
        },
        SPLITLEFT => {
            ewmh_restore(ewmh, active_window, screen)?;
            match window_properties.get_mut(&active_window) {
                Some(prop) => prop.state = State::SplitLeft,
                None => return Err(GenericError::new("cannot find active window in memory")),
            }
            move_resize(base, ewmh, active_window, work_x, work_y, half_width, work_height)?;
        },
        SPLITRIGHT => {
            ewmh_restore(ewmh, active_window, screen)?;
            match window_properties.get_mut(&active_window) {
                Some(prop) => prop.state = State::SplitRight,
                None => return Err(GenericError::new("cannot find active window in memory")),
            }
            move_resize(base, ewmh, active_window, half_width as i16, work_y, half_width, work_height)?;
        },
        MAXIMIZE => {
            if let Some(prop) = window_properties.get_mut(&active_window) {
                prop.state = State::Maximized;
            }
            ewmh_maximize(ewmh, active_window, screen)?;
        },
        SAVE => {
            let prop = Properties{state: State::Windowed, dimensions: current_dimensions};
            window_properties.insert(active_window, prop);
        },
        _ => return Err(GenericError::new("invalid command")),
    }
    Ok(())
}

fn exit() {
    // We should gracefully handle each operation so that everything gets executed
    if let Err(e) = remove_socket_file() {
        eprintln!("{}", e);
    }
}

fn event_loop() -> Result<(), GenericError> {
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

        match message.get(COMMAND).unwrap() {
            RESTART => {
                connections = setup_connections()?;
                continue;
            },
            QUIT => break,
            _ => {
                if let Err(e) = do_single_command(&connections, &mut window_properties, message) {
                    eprintln!("{}", e);
                }
            },
        }
    }
    Ok(())
}

fn main() {
    if let Err(e) = event_loop() {
        eprintln!("{}", e);
    }
    exit();
}
