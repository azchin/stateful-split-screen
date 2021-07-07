pub mod errors {
    use std::error::Error;
    use std::fmt;

    #[derive(Debug)]
    pub struct GenericError {
        details: String,
    }

    impl GenericError {
        pub fn new(details: &str) -> GenericError {
            GenericError{details: details.to_string()}
        }
    }

    impl Error for GenericError {
    }

    impl fmt::Display for GenericError {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "ERROR: {}", self.details)
        }
    }
}

pub mod commands {
    pub const COMMAND: &str = "command";
    pub const RESTORE: &str = "restore";
    pub const SPLITLEFT: &str = "splitleft";
    pub const SPLITRIGHT: &str = "splitright";
    pub const RESTART: &str = "restart";
    pub const QUIT: &str = "quit";
}

pub mod data {
    use std::collections::HashMap;
    use byteorder::LE;
    use zvariant::{from_slice, to_bytes};
    use zvariant::EncodingContext as Context;
    use crate::errors::GenericError;

    pub struct Message {
        map: HashMap<String, String>,
    }

    impl Message {
        pub fn new() -> Message {
            let map = HashMap::new();
            Message{map}
        }
        pub fn get(&self, key: &str) -> Option<&str> {
            match self.map.get(key) {
                Some(res) => Some(res.as_str()),
                None => None,
            }
        }
        pub fn insert(&mut self, key: &str, value: &str) {
            self.map.insert(key.to_string(), value.to_string());
        }
        fn from_expose(map: HashMap<String, String>) -> Message {
            Message{map}
        }
        fn expose(&mut self) -> HashMap<String, String> {
            std::mem::replace(&mut self.map, HashMap::new())
        }
    }

    pub fn encode_data(mut message: Message) -> Result<Vec<u8>, GenericError> {
        let ctxt = Context::<LE>::new_gvariant(0);
        match to_bytes(ctxt, &message.expose()) {
            Ok(res) => Ok(res),
            Err(_) => return Err(GenericError::new("gvariant encoding")),
        }
    }

    pub fn decode_data(binary: &[u8]) -> Result<Message, GenericError> {
        let ctxt = Context::<LE>::new_gvariant(0);
        match from_slice(&binary, ctxt) {
            Ok(res) => Ok(Message::from_expose(res)),
            Err(_) => return Err(GenericError::new("gvariant decoding")),
        }
    }
}

pub mod socket {
    use crate::errors::GenericError;
    use std::os::unix::net::UnixDatagram;
    use std::path::PathBuf;
    use std::fs;

    fn get_socket_dir() -> Result<PathBuf, GenericError> {
        if let Some(mut cachepath) = dirs::cache_dir() {
            cachepath.push("sss_socket");
            Ok(cachepath)
        }
        else if let Some(mut homepath) = dirs::home_dir() {
            homepath.push(".sss_socket");
            Ok(homepath)
        }
        else {
            Err(GenericError::new("getting cache or home directory"))
        }
    }

    pub fn get_socket_file() -> Result<PathBuf, GenericError> {
        let socket_path = get_socket_dir()?;
        match socket_path.exists() {
            true => Ok(socket_path),
            _ => Err(GenericError::new("socket does not exists")),
        }
    }

    pub fn remove_socket_file() -> Result<(), GenericError> {
        let socket_path = get_socket_dir()?;
        if socket_path.exists() {
            if let Err(_) = fs::remove_file(socket_path.as_path()) {
                return Err(GenericError::new("cannot remove old socket"));
            }
        }
        Ok(())
    }

    // TODO modify permissions, race condition potential btw
    pub fn bind_socket() -> Result<UnixDatagram, GenericError> {
        let socket_path = get_socket_dir()?;
        remove_socket_file()?;
        match UnixDatagram::bind(socket_path.as_path()) {
            Ok(sock) => Ok(sock),
            Err(_) => Err(GenericError::new("socket binding")),
        }
    }
}

pub mod xcb {
    use crate::errors::GenericError;
    use xcb_util::ewmh;
    use xcb::base;
    use xcb::xproto;
    
    pub struct XCBConnections {
        pub base: base::Connection,
        pub ewmh: ewmh::Connection,
        pub screen: i32,
    }
    
    pub fn get_root_window(base: &base::Connection, screen: i32) -> Result<xproto::Window, GenericError> {
        let setup = base.get_setup();
        match setup.roots().nth(screen as usize) {
            Some(screen) => Ok(screen.root()),
            None => Err(GenericError::new("iterating through screens")),
        }
    }

    pub fn get_active_window(ewmh: &ewmh::Connection, screen: i32) -> Result<xproto::Window, GenericError> {
        let window_cookie = ewmh::get_active_window(ewmh, screen);
        let window_res = window_cookie.get_reply();
        match window_res {
            Ok(window) => Ok(window),
            Err(_) => Err(GenericError::new("get active window")),
        }
    }

    pub fn get_parent_window(base: &base::Connection, window: xproto::Window) -> Result<xproto::Window, GenericError> {
        let query_cookie = xproto::query_tree(base, window);
        match query_cookie.get_reply() {
            Ok(tree) => Ok(tree.parent()),
            Err(_) => Err(GenericError::new("query tree")),
        }
    }

    fn get_extents(ewmh: &ewmh::Connection, window: xproto::Window) -> Result<(u32, u32, u32, u32), GenericError> {
        let extent_cookie = ewmh::get_frame_extents(ewmh, window);
        let extent = match extent_cookie.get_reply() {
            Ok(res) => res,
            Err(_) => return Err(GenericError::new("get frame extents")),
        };

        // For some reason, these signify (left, right, top, bottom).
        //   xprop exhibits this order properly, so I don't know whether it's a Rust binding issue
        //   or an xcb issue.
        Ok((extent.top(), extent.bottom(), extent.left(), extent.right()))
        // Ok((extent.left(), extent.right(), extent.top(), extent.bottom()))
    }

    pub fn get_geometry(base: &base::Connection, ewmh: &ewmh::Connection, window: xproto::Window) -> Result<(i16, i16, u16, u16), GenericError> {
        let geo_cookie = xproto::get_geometry(base, window);
        let geo = match geo_cookie.get_reply() {
            Ok(reply) => reply,
            Err(_) => return Err(GenericError::new("get window geometry")),
        };

        // let parent = get_parent_window(base, window)?;
        let translate_cookie = xproto::translate_coordinates(&base, window, geo.root(), geo.x(), geo.y());
        let translate = match translate_cookie.get_reply() {
            Ok(res) => res,
            Err(_) => return Err(GenericError::new("translate parent's coordinates")),
        };
        let (ext_left, ext_right, ext_top, ext_bottom) = get_extents(ewmh, window)?;
        Ok((
            translate.dst_x() - 2 * ext_left as i16,
            translate.dst_y() - 2 * ext_top as i16,
            geo.width() + (ext_left + ext_right) as u16,
            geo.height() + (ext_top + ext_bottom) as u16,
        ))
    }

    pub fn get_desktop_geometry(ewmh: &ewmh::Connection, screen: i32) -> Result<(u32, u32), GenericError> {
        let desktop_cookie = ewmh::get_desktop_geometry(ewmh, screen);
        match desktop_cookie.get_reply() {
            Ok(res) => Ok(res),
            Err(_) => Err(GenericError::new("get desktop geometry")),
        }
    }

    fn get_desktop_idx(ewmh: &ewmh::Connection, screen: i32) -> Result<u32, GenericError> {
        let desktop_cookie = ewmh::get_current_desktop(ewmh, screen);
        match desktop_cookie.get_reply() {
            Ok(res) => Ok(res),
            Err(_) => Err(GenericError::new("get current desktop")),
        }
    }

    pub fn get_work_area(ewmh: &ewmh::Connection, screen: i32) -> Result<(u32, u32, u32, u32), GenericError> {
        let area_cookie = ewmh::get_work_area(ewmh, screen);
        let areas = match area_cookie.get_reply() {
            Ok(res) => res,
            Err(_) => return Err(GenericError::new("get work area")),
        };
        let idx = get_desktop_idx(ewmh, screen)? as usize;
        match areas.work_area().get(idx) {
            Some(area) => Ok((area.x(), area.y(), area.width(), area.height())),
            None => Err(GenericError::new("couldn't find work area for screen")),
        }
    }

    pub fn move_resize(base: &base::Connection, ewmh: &ewmh::Connection, window: xproto::Window,
                       x: i16, y: i16, width: u16, height: u16) -> Result<(), GenericError> {
        let (ext_left, ext_right, ext_top, ext_bottom) = get_extents(ewmh, window)?;
        let width = width - (ext_left + ext_right) as u16;
        let height = height - (ext_top + ext_bottom) as u16;
        let value_list = [
            (xproto::CONFIG_WINDOW_X as u16, x as u32),
            (xproto::CONFIG_WINDOW_Y as u16, y as u32),
            (xproto::CONFIG_WINDOW_WIDTH as u16, width as u32),
            (xproto::CONFIG_WINDOW_HEIGHT as u16, height as u32),
        ];
        // let value_list = [(200, xproto::CONFIG_WINDOW_HEIGHT)];
        let cookie = xproto::configure_window(base, window, &value_list);
        match cookie.request_check() {
            Ok(_) => Ok(()),
            Err(_) => Err(GenericError::new("move and resize window")),
        }
    }

    pub fn setup_connections() -> Result<XCBConnections, GenericError> {
        let base_connection_res = base::Connection::connect(None);
        let (base_connection, default_screen);
        match base_connection_res {
            Ok((conn, screen)) => {
                base_connection = conn;
                default_screen = screen;
            },
            _ => return Err(GenericError::new("XCB connection #1")),
        }
        let ewmh_connection_res = ewmh::Connection::connect(base_connection);
        let ewmh_connection;
        match ewmh_connection_res {
            Ok(conn) => ewmh_connection = conn,
            _ => return Err(GenericError::new("XCB EWMH connection")),
        }

        // For now we do all of this again to re-establish base_connection
        let base_connection_res = base::Connection::connect(None);
        let base_connection;
        match base_connection_res {
            Ok((conn, _screen)) => {
                base_connection = conn;
            },
            _ => return Err(GenericError::new("XCB connection #2")),
        }
        Ok(XCBConnections{base: base_connection, ewmh: ewmh_connection, screen: default_screen})
    }
}
