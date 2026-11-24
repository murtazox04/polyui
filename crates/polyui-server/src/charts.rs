use polyui_chart::{ChartOptions, ChartSpec, compile, describe};
use polyui_core::pointer;
use polyui_core::protocol::Component;
use polyui_core::validate::{Location, Problem};
use serde_json::{Map, Value};

pub const CHART_TYPE: &str = "chart:Chart";
pub const SCENE_PROP: &str = "scene";

fn literal_props(props: &Map<String, Value>, data: &Value) -> Map<String, Value> {
    props
        .iter()
        .filter_map(|(name, value)| {
            let bound = value
                .as_object()
                .filter(|map| map.len() == 1)
                .and_then(|map| map.get("$bind"))
                .and_then(Value::as_str);
            match bound {
                Some(path) => pointer::get(data, path).map(|found| (name.clone(), found.clone())),
                None => Some((name.clone(), value.clone())),
            }
        })
        .collect()
}

pub fn attach_scenes(
    components: &mut [Component],
    data: &Value,
    list: &str,
    options: &ChartOptions,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (index, component) in components.iter_mut().enumerate() {
        if component.type_name != CHART_TYPE {
            continue;
        }
        let location = Location::Item {
            list: list.to_string(),
            index,
            id: Some(component.id.clone()),
            prop: None,
        };
        let props = literal_props(&component.props, data);
        let spec = match serde_json::from_value::<ChartSpec>(Value::Object(props)) {
            Ok(spec) => spec,
            Err(error) => {
                problems.push(Problem::new(
                    location,
                    format!("chart props could not be read ({error}). Write title, kind and rows inline."),
                ));
                continue;
            }
        };
        match compile(&spec, options) {
            Ok(scene) => match serde_json::to_value(&scene) {
                Ok(scene) => {
                    component.props.insert(SCENE_PROP.into(), scene);
                    if component.say.is_none() {
                        component.say = Some(describe(&spec, options.locale));
                    }
                }
                Err(error) => problems.push(Problem::new(
                    location,
                    format!("chart could not be drawn: {error}."),
                )),
            },
            Err(error) => problems.push(Problem::new(
                location,
                format!("chart could not be drawn: {error}."),
            )),
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scene_and_spoken_summary_are_added() {
        let mut components: Vec<Component> = serde_json::from_value(json!([
            { "id": "c", "type": "chart:Chart", "props": { "title": "Gas", "kind": "bar", "unit": "m3", "rows": [{ "x": "Jan", "y": 3 }, { "x": "Feb", "y": 5 }] } },
            { "id": "t", "type": "Text", "props": { "text": "hi" } }
        ]))
        .unwrap();
        let problems = attach_scenes(
            &mut components,
            &json!({}),
            "components",
            &ChartOptions::default(),
        );
        assert!(problems.is_empty());
        assert!(components[0].props[SCENE_PROP]["marks"].is_array());
        assert!(
            components[0]
                .say
                .as_deref()
                .unwrap_or_default()
                .starts_with("Gas.")
        );
        assert!(components[1].props.get(SCENE_PROP).is_none());
    }

    #[test]
    fn missing_bound_rows_draw_an_empty_chart() {
        let mut components: Vec<Component> = serde_json::from_value(json!([
            { "id": "c", "type": "chart:Chart", "props": { "title": "Gas", "kind": "bar", "rows": { "$bind": "/nothing" } } }
        ]))
        .unwrap();
        let problems = attach_scenes(
            &mut components,
            &json!({}),
            "components",
            &ChartOptions::default(),
        );
        assert_eq!(problems.len(), 0);
        assert!(components[0].props.contains_key(SCENE_PROP));
    }
}
