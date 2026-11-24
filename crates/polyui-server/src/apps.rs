use crate::host::{Host, User};
use polyui_core::Limits;
use polyui_core::embed::{self, MAX_APPS, UiResource};
use polyui_core::protocol::Component;
use polyui_core::validate::Problem;
use std::collections::BTreeMap;

pub use polyui_core::client::{APP_PROP, EMBED_TYPE, MCP_APP_MIME_TYPE};

pub type Apps = BTreeMap<String, Option<UiResource>>;

pub async fn read_apps<I>(host: &dyn Host, user: &User, uris: I, apps: &mut Apps)
where
    I: IntoIterator<Item = String>,
{
    for uri in uris {
        if apps.contains_key(&uri) || apps.len() >= MAX_APPS {
            continue;
        }
        let resource = host.ui_resource(user, &uri).await;
        apps.insert(uri, resource);
    }
}

pub fn attach_apps(
    components: &mut [Component],
    apps: &Apps,
    list: &str,
    limits: &Limits,
) -> Vec<Problem> {
    embed::attach_apps(components, list, limits, &mut |uri| {
        apps.get(uri).cloned().flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apps_are_filled_from_what_was_read() {
        let uri = "ui://bills/tariff";
        let mut components: Vec<Component> = serde_json::from_value(json!([
            { "id": "calc", "type": EMBED_TYPE, "props": { "resource": uri, "title": "Tariffs" } },
            { "id": "other", "type": EMBED_TYPE, "props": { "resource": "ui://bills/unread", "title": "Other" } },
            { "id": "t", "type": "Text", "props": { "text": "hi" } }
        ]))
        .unwrap();
        let apps = Apps::from([(
            uri.to_string(),
            Some(UiResource::new(
                json!({ "uri": uri, "mimeType": MCP_APP_MIME_TYPE, "text": "<p>calc</p>" }),
            )),
        )]);
        let problems = attach_apps(&mut components, &apps, "components", &Limits::default());
        assert_eq!(
            components[0].props[APP_PROP]["resource"]["text"],
            "<p>calc</p>"
        );
        assert_eq!(problems.len(), 1);
        assert!(
            problems[0]
                .to_string()
                .starts_with("components[1] id \"other\"")
        );
        assert!(components[2].props.get(APP_PROP).is_none());
    }
}
