
#[derive(Debug, Deserialize)]
pub struct PodManifest {
    pub apps: Vec<RuntimeApp>,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeApp {
    pub name: String,
    pub app: App,
}

#[derive(Debug, Deserialize)]
pub struct App {
    pub exec: Vec<String>,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub environment: Vec<NameValue>,
    #[serde(rename = "workingDirectory", default)]
    pub working_directory: String,
}

#[derive(Debug, Deserialize)]
pub struct NameValue {
    pub name: String,
    pub value: String,
}
