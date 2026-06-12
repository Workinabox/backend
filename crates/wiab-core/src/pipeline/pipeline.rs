use crate::pipeline::{PipelineError, PipelineId, PipelineSnapshot};
use crate::project::ProjectId;

/// A pipeline: a `PL-###` id, the project it belongs to, a name, and a description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pipeline {
    id: PipelineId,
    project_id: ProjectId,
    name: String,
    description: String,
}

impl Pipeline {
    pub fn new(
        id: PipelineId,
        project_id: ProjectId,
        name: String,
        description: String,
    ) -> Result<Self, PipelineError> {
        if name.trim().is_empty() {
            return Err(PipelineError::EmptyName);
        }
        Ok(Self {
            id,
            project_id,
            name,
            description,
        })
    }

    pub fn id(&self) -> PipelineId {
        self.id
    }

    pub fn project_id(&self) -> ProjectId {
        self.project_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn update(&mut self, name: String, description: String) -> Result<(), PipelineError> {
        if name.trim().is_empty() {
            return Err(PipelineError::EmptyName);
        }
        self.name = name;
        self.description = description;
        Ok(())
    }

    pub fn snapshot(&self) -> PipelineSnapshot {
        PipelineSnapshot {
            id: self.id.to_string(),
            project_id: self.project_id.to_string(),
            name: self.name.clone(),
            description: self.description.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pipeline(number: u64, name: &str) -> Pipeline {
        Pipeline::new(
            PipelineId::from_number(number),
            ProjectId::from_number(1),
            name.to_owned(),
            String::new(),
        )
        .unwrap()
    }

    #[test]
    fn rejects_empty_name() {
        let error = Pipeline::new(
            PipelineId::from_number(1),
            ProjectId::from_number(1),
            "  ".to_owned(),
            String::new(),
        )
        .unwrap_err();
        assert_eq!(error, PipelineError::EmptyName);
    }

    #[test]
    fn exposes_getters() {
        let pipeline = Pipeline::new(
            PipelineId::from_number(1),
            ProjectId::from_number(2),
            "Deploy".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        assert_eq!(pipeline.id(), PipelineId::from_number(1));
        assert_eq!(pipeline.project_id(), ProjectId::from_number(2));
        assert_eq!(pipeline.name(), "Deploy");
        assert_eq!(pipeline.description(), "desc");
    }

    #[test]
    fn update_replaces_name_and_description_but_not_project() {
        let mut pipeline = pipeline(1, "Deploy");
        pipeline
            .update("Release".to_owned(), "to prod".to_owned())
            .unwrap();
        assert_eq!(pipeline.name(), "Release");
        assert_eq!(pipeline.description(), "to prod");
        assert_eq!(pipeline.project_id(), ProjectId::from_number(1));
    }

    #[test]
    fn update_rejects_empty_name() {
        let mut pipeline = pipeline(1, "Deploy");
        let error = pipeline
            .update("  ".to_owned(), "to prod".to_owned())
            .unwrap_err();
        assert_eq!(error, PipelineError::EmptyName);
        assert_eq!(pipeline.name(), "Deploy");
        assert_eq!(pipeline.description(), "");
    }

    #[test]
    fn snapshot_mirrors_fields() {
        let pipeline = Pipeline::new(
            PipelineId::from_number(1),
            ProjectId::from_number(2),
            "Deploy".to_owned(),
            "desc".to_owned(),
        )
        .unwrap();
        let snapshot = pipeline.snapshot();
        assert_eq!(snapshot.id, "PL-1");
        assert_eq!(snapshot.project_id, "P-2");
        assert_eq!(snapshot.name, "Deploy");
        assert_eq!(snapshot.description, "desc");
    }
}
