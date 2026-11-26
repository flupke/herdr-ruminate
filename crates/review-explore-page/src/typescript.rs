use std::collections::BTreeMap;

use ts_rs::{Config, TS, TypeVisitor};

use crate::rpc::{Call, Notification, Reply, Request};

const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/client/types.ts");

// The order in which ts-rs visits the types changes from one build to the next.
fn declarations() -> String {
    let mut collected = Declarations {
        config: Config::new().with_large_int("number"),
        declared: BTreeMap::new(),
    };
    collected.visit::<Notification>();
    collected.visit::<Request>();
    collected.visit::<Call>();
    collected.visit::<Reply>();
    let mut lines = vec![
        "// The types of the messages of the Explore page's socket. Generated from the Rust types by \
         `make explore-types`: do not edit."
            .to_owned(),
    ];
    lines.extend(collected.declared.into_values());
    lines.push(String::new());
    lines.join("\n")
}

struct Declarations {
    config: Config,
    declared: BTreeMap<String, String>,
}

impl TypeVisitor for Declarations {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        // ts-rs declares only the types that derive `TS`; a wrapper (`Option`, `Vec`) declares its
        // contents.
        if T::output_path().is_some() {
            let name = T::ident(&self.config);
            if self.declared.contains_key(&name) {
                return;
            }
            let docs = T::docs()
                .map(|docs| format!("{}\n", docs.trim_end()))
                .unwrap_or_default();
            let declaration = format!("{docs}export {}", T::decl(&self.config));
            self.declared.insert(name, declaration);
        }
        T::visit_dependencies(self);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clients_types_are_those_of_the_socket_messages() {
        let generated = declarations();
        if std::env::var("EXPLORE_TYPES").as_deref() == Ok("write") {
            std::fs::write(FILE, &generated).unwrap();
            return;
        }
        let committed = std::fs::read_to_string(FILE).unwrap_or_default();
        assert!(
            committed == generated,
            "{FILE} does not match the Rust types of the socket's messages: run `make explore-types`"
        );
    }
}
