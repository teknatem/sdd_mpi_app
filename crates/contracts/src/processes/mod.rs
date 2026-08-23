//! Контракты механизма Процессов (ADR-0011).
//!
//! Термины — в `CONTEXT.md`, раздел «Механизм процессов». Коротко: Процесс
//! (`pr0001`) — граф Этапов; Этап (`st0001`) — mjs-модуль с именованными
//! выходами; Действие — операция ядра с побочным эффектом, вызываемая с Этапа.
//!
//! Слои построены снизу вверх и в этом порядке имеют смысл поодиночке:
//! Действие с журналом эффектов, Этап с контрактом выходов, Процесс как граф
//! над Этапами и хранимое определение с версией под ними обоими.

pub mod action;
pub mod definition;
pub mod event;
pub mod instance;
pub mod process;
pub mod stage;

pub use action::{
    ActionActor, ActionCall, ActionInfo, ActionMode, ActionOutcome, EffectRecord, EffectStatus,
};
pub use definition::{
    ActivationPlan, DefinitionDiff, DefinitionRecord, DefinitionStatus, DefinitionVersion, StagePin,
};
pub use event::{CorrelationKey, DomainEvent, DomainEventKind};
pub use instance::{InstanceDetails, InstanceStatus, InstanceStep, InstanceWait, ProcessInstance};
pub use process::{
    EdgeTarget, ProcessCriticality, ProcessDefinition, ProcessEdge, ProcessManifest,
    ProcessTrigger, WaitSpec,
};
pub use stage::{
    StageDefinition, StageManifest, StageOutcome, StageOutput, StageRun, StageRunContext,
    StageVerdict,
};

/// Хранимая версия Этапа.
pub type StageRecord = DefinitionRecord<StageDefinition>;
/// Хранимая версия Процесса.
pub type ProcessRecord = DefinitionRecord<ProcessDefinition>;
