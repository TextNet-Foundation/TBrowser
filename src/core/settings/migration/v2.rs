use crate::core::settings::{Settings, Value};

pub fn migrate(from: u32, settings: &mut Settings) -> bool {
    if from > 1 {
        return true;
    }

    rename(
        settings,
        "network.allow_legacy_1_0",
        "network.allow_protocols_v1",
    );

    rename(
        settings,
        "network.protocols.allow_legacy_1_0",
        "network.allow_protocols_v1",
    );

    if !matches!(
        settings.get("network.allow_protocols_v1"),
        Some(Value::Bool(_))
    ) {
        settings
            .values
            .insert("network.allow_protocols_v1".to_string(), Value::Bool(false));
    }

    true
}

fn rename(settings: &mut Settings, old: &str, new: &str) {
    if settings.get(new).is_some() {
        settings.values.remove(old);
        return;
    }

    if let Some(value) = settings.values.remove(old) {
        settings.values.insert(new.to_string(), value);
    }
}
