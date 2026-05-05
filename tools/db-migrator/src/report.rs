use serde::Serialize;

#[derive(Debug, Clone, Serialize, Default)]
pub struct MigrationReport {
    #[serde(rename = "runId")]
    pub run_id: String,
    pub domains: Vec<DomainReport>,
    pub warnings: Vec<ReportIssue>,
    pub errors: Vec<ReportIssue>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DomainReport {
    pub name: String,
    pub planned: usize,
    pub created: usize,
    pub updated: usize,
    pub skipped: usize,
    pub warnings: usize,
    pub errors: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportIssue {
    pub domain: String,
    pub entity_type: String,
    pub source_id: String,
    pub message: String,
}

impl MigrationReport {
    pub fn new(run_id: String) -> Self {
        Self {
            run_id,
            ..Self::default()
        }
    }

    pub fn domain_mut(&mut self, name: &str) -> &mut DomainReport {
        if let Some(index) = self.domains.iter().position(|item| item.name == name) {
            return &mut self.domains[index];
        }

        self.domains.push(DomainReport {
            name: name.to_string(),
            ..DomainReport::default()
        });
        self.domains.last_mut().expect("domain report exists")
    }

    pub fn warning(
        &mut self,
        domain: &str,
        entity_type: &str,
        source_id: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.domain_mut(domain).warnings += 1;
        self.warnings.push(ReportIssue {
            domain: domain.to_string(),
            entity_type: entity_type.to_string(),
            source_id: source_id.into(),
            message: message.into(),
        });
    }

    #[allow(dead_code)]
    pub fn error(
        &mut self,
        domain: &str,
        entity_type: &str,
        source_id: impl Into<String>,
        message: impl Into<String>,
    ) {
        self.domain_mut(domain).errors += 1;
        self.errors.push(ReportIssue {
            domain: domain.to_string(),
            entity_type: entity_type.to_string(),
            source_id: source_id.into(),
            message: message.into(),
        });
    }
}
