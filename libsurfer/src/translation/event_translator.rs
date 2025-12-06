use surfer_translation_types::{
    TranslationResult, Translator, VariableInfo, VariableType, VariableValue,
};

use crate::message::Message;
use crate::translation::{AnyTranslator, BitTranslator};
use crate::wave_container::{ScopeId, VarId, VariableMeta};

pub struct EventTranslator {
    // In order to not duplicate logic, we reuse the bit translator internally
    inner: AnyTranslator,
}

impl EventTranslator {
    pub fn new() -> Self {
        Self {
            inner: AnyTranslator::Basic(Box::new(BitTranslator {})),
        }
    }
}

impl Default for EventTranslator {
    fn default() -> Self {
        Self::new()
    }
}

impl Translator<VarId, ScopeId, Message> for EventTranslator {
    fn name(&self) -> String {
        "Event".to_string()
    }

    fn translate(
        &self,
        variable: &VariableMeta,
        value: &VariableValue,
    ) -> eyre::Result<TranslationResult> {
        self.inner.translate(variable, value)
    }

    fn variable_info(&self, _variable: &VariableMeta) -> eyre::Result<VariableInfo> {
        Ok(VariableInfo::Event)
    }

    fn translates(&self, variable: &VariableMeta) -> eyre::Result<super::TranslationPreference> {
        if variable.num_bits == Some(1) {
            match &variable.variable_type {
                Some(VariableType::VCDEvent) => Ok(super::TranslationPreference::Prefer),
                _ => Ok(super::TranslationPreference::Yes),
            }
        } else {
            Ok(super::TranslationPreference::No)
        }
    }
}
