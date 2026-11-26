use crate::Settings;
use crate::core::NodeDef;
use crate::core::Value;


pub mod client_errors;
pub mod appearance;
pub mod general;
pub mod network;
pub mod render_themes;

use self::client_errors::NODE_CLIENT_ERRORS;
use self::render_themes::NODE_RENDER_THEMES;

pub const CATEGORIES: &[NodeDef] = &[
    general::NODE_GENERAL,
    appearance::NODE_APPEARANCE,
    network::NODE_NETWORK,
    NODE_CLIENT_ERRORS,
    NODE_RENDER_THEMES,
];

pub fn when_x_value(settings: &Settings, key: String, value: &Value) -> bool {
    match settings.get(&key) {
        Some(val) => value == val,
        _ => false,
    }
}