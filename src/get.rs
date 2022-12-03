use minijinja::Environment;
use serde_json::{from_str, Value};

pub fn table(table: String) -> String {
    let mut env = Environment::new();
    env.add_template("page.html", include_str!("resources/page.html"))
        .unwrap();
    env.add_template("table.html", include_str!("resources/table.html"))
        .unwrap();

    let data: Value = from_str(include_str!("resources/page.json")).unwrap();
    let title: &str = data
        .get("page")
        .and_then(|value| value.get("title"))
        .and_then(|value| value.as_str())
        .unwrap();
    tracing::info!("format: {:?}", title);
    let template = env.get_template("table.html").unwrap();
    template.render(data).unwrap()
}
