use crate::lsp::ZedLspSupport;
use std::collections::BTreeMap;

#[derive(Default)]
pub struct ZedOxfmtLsp {
    sources: BTreeMap<u64, bool>,
}

impl ZedLspSupport for ZedOxfmtLsp {
    fn package_name(&self) -> &'static str {
        "oxfmt"
    }

    fn sources(&self) -> &BTreeMap<u64, bool> {
        &self.sources
    }

    fn sources_mut(&mut self) -> &mut BTreeMap<u64, bool> {
        &mut self.sources
    }
}
