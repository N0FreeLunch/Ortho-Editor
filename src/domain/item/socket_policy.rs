//! Source socket limits selected from ordered item types and an explicit item level.

use crate::data::legitimacy::SOCKET_RULES;

pub(crate) fn source_socket_limit(types: Vec<&str>, ilvl: u8) -> Option<u8> {
    let mut max_sockets = None;

    for type_code in types {
        if let Some(rule) = SOCKET_RULES.iter().find(|r| r.item_type == type_code) {
            let max = if ilvl >= 41 {
                rule.max_sock_high
            } else if ilvl >= 26 {
                rule.max_sock_mid
            } else {
                rule.max_sock_low
            };
            max_sockets = Some(max);
            break;
        }
    }

    max_sockets
}
