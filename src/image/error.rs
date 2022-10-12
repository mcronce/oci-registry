#[derive(Debug, thiserror::Error)]
#[error("Invalid image name '{0}'")]
pub struct InvalidImageName(pub String);

#[derive(Debug, thiserror::Error)]
#[error("Invalid image reference '{0}'")]
pub struct InvalidImageReference(pub String);

