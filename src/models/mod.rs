pub mod event;
pub mod training;

pub use event::{
    Event, EventPosition, EventTmi, OpsPlanFile, CreateEventRequest, UpdateEventRequest,
    CreateEventPositionRequest, AssignEventPositionRequest,
};

pub use training::{
    CreateTrainerReleaseRequestRequest, CreateTrainingAssignmentRequest,
    CreateTrainingAssignmentRequestRequest, DecideTrainerReleaseRequestRequest,
    DecideTrainingAssignmentRequestRequest, TrainerReleaseRequest, TrainingAssignment,
    TrainingAssignmentRequest,
};

