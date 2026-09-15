use crate::lsp::ZedLspSupport;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct ZedOxlintLsp {
    sources: BTreeMap<u64, bool>,
}

impl ZedLspSupport for ZedOxlintLsp {
    fn package_name(&self) -> &'static str {
        "oxlint"
    }

    fn sources(&self) -> &BTreeMap<u64, bool> {
        &self.sources
    }

    fn sources_mut(&mut self) -> &mut BTreeMap<u64, bool> {
        &mut self.sources
    }
}
