#[cfg(test)]
mod adapter_tests {
    use crate::{AdapterRegistry, FeatureReport, LanguageAdapter};
    use crush_cast::Program;

    struct MockAdapter {
        lang: &'static str,
        exts: &'static [&'static str],
    }

    impl LanguageAdapter for MockAdapter {
        fn language_name(&self) -> &'static str { self.lang }
        fn file_extensions(&self) -> &[&'static str] { self.exts }
        fn walk(&self, _source: &str, _filename: &str) -> anyhow::Result<(FeatureReport, Program)> {
            Ok((FeatureReport { lang: self.lang.to_string(), ..Default::default() }, Program::default()))
        }
    }

    #[test]
    fn test_registry_walk_by_extension() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter { lang: "testlang", exts: &["tl", "test"] }));

        let (report, _program) = registry.walk("dummy source", "hello.tl").unwrap();
        assert_eq!(report.lang, "testlang");
    }

    #[test]
    fn test_registry_walk_unknown_extension() {
        let registry = AdapterRegistry::new();
        let result = registry.walk("source", "file.unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("no walker registered"));
    }

    #[test]
    fn test_registry_multiple_adapters() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter { lang: "python", exts: &["py"] }));
        registry.register(Box::new(MockAdapter { lang: "rust", exts: &["rs"] }));

        let (r1, _) = registry.walk("x = 1", "test.py").unwrap();
        let (r2, _) = registry.walk("fn main(){}", "test.rs").unwrap();
        assert_eq!(r1.lang, "python");
        assert_eq!(r2.lang, "rust");
    }

    #[test]
    fn test_registry_can_handle() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter { lang: "go", exts: &["go"] }));

        assert!(registry.walk("", "test.go").is_ok());
        assert!(registry.walk("", "test.rs").is_err());
    }

    #[test]
    fn test_registry_languages() {
        let mut registry = AdapterRegistry::new();
        registry.register(Box::new(MockAdapter { lang: "a", exts: &["a"] }));
        registry.register(Box::new(MockAdapter { lang: "b", exts: &["b"] }));

        let langs = registry.languages();
        assert!(langs.contains(&"a"));
        assert!(langs.contains(&"b"));
    }
}
