#[allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
#[allow(non_upper_case_globals, non_snake_case, unused_imports, missing_docs, clippy::all)]
pub mod zclear_inspector_v1 {
    pub mod client {
        use wayland_client;
        use wayland_client::protocol::*;

        pub mod __interfaces {
            use wayland_client::protocol::__interfaces::*;
            wayland_scanner::generate_interfaces!("protocol/clear-inspector-v1.xml");
        }
        use self::__interfaces::*;

        wayland_scanner::generate_client_code!("protocol/clear-inspector-v1.xml");
    }
    pub use self::client::zclear_inspector_v1::*;
}
