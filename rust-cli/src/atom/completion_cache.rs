#[derive(Debug, Default, Clone)]
pub(crate) struct CompletionCache {
    valid: bool,
    variables: Vec<String>,
    macros: Vec<String>,
}

impl CompletionCache {
    pub(crate) fn invalidate(&mut self) {
        self.valid = false;
        self.variables.clear();
        self.macros.clear();
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.valid
    }

    pub(crate) fn variables(&self) -> &[String] {
        &self.variables
    }

    pub(crate) fn macros(&self) -> &[String] {
        &self.macros
    }

    pub(crate) fn update(&mut self, variables: Vec<String>, macros: Vec<String>) {
        self.valid = true;
        self.variables = variables;
        self.macros = macros;
    }
}

#[cfg(test)]
mod tests {
    use super::CompletionCache;

    #[test]
    fn cache_updates_and_invalidates() {
        let mut cache = CompletionCache::default();
        assert!(!cache.is_valid());

        cache.update(vec!["iq".to_string()], vec!["sample_macro".to_string()]);
        assert!(cache.is_valid());
        assert_eq!(cache.variables(), ["iq"]);
        assert_eq!(cache.macros(), ["sample_macro"]);

        cache.invalidate();
        assert!(!cache.is_valid());
        assert!(cache.variables().is_empty());
        assert!(cache.macros().is_empty());
    }
}
