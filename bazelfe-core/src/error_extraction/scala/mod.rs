use std::{collections::HashMap, path::Path};

use crate::source_dependencies::ParsedFile;

mod error_is_not_a_member_of_package;
mod error_object_not_found;
mod error_symbol_is_missing_from_classpath;
mod error_symbol_type_missing_from_classpath;
mod error_type_not_found;
mod error_value_not_found;

#[derive(Debug, PartialEq, Clone)]
pub struct ScalaClassImportRequest {
    pub src_file_name: String,
    pub class_name: String,
    pub exact_only: bool,
    pub src_fn: &'static str,
    pub priority: i32,
}

impl ScalaClassImportRequest {
    pub fn to_class_import_request(self) -> super::ClassImportRequest {
        super::ClassImportRequest {
            class_name: self.class_name,
            exact_only: self.exact_only,
            src_fn: format!("scala::{}", self.src_fn),
            priority: self.priority,
        }
    }
}

fn do_load_file(path_str: &str) -> Option<ParsedFile> {
    let path = Path::new(path_str);

    if path.exists() {
        let file_contents = std::fs::read_to_string(path).unwrap();
        match crate::source_dependencies::scala::parse_file(&file_contents) {
            Err(_) => None,
            Ok(file) => Some(file),
        }
    } else {
        None
    }
}

pub(in crate::error_extraction) struct FileParseCache {
    file_parse_cache: HashMap<String, ParsedFile>,
}
impl FileParseCache {
    pub fn new() -> Self {
        Self {
            file_parse_cache: HashMap::new(),
        }
    }
    // used in tests
    #[allow(dead_code)]
    pub fn init_from_par(key: String, v: ParsedFile) -> Self {
        let mut map = HashMap::new();
        map.insert(key, v);
        Self {
            file_parse_cache: map,
        }
    }
    pub fn load_file(&mut self, file_path: &str) -> Option<&ParsedFile> {
        if !self.file_parse_cache.contains_key(file_path) {
            if let Some(parsed_file) = do_load_file(file_path) {
                self.file_parse_cache
                    .insert(file_path.to_string(), parsed_file);
            }
        }
        self.file_parse_cache.get(file_path)
    }
}

pub fn extract_errors(input: &str) -> Vec<super::ClassImportRequest> {
    let mut file_parse_cache: FileParseCache = FileParseCache::new();

    let combined_vec: Vec<super::ClassImportRequest> = vec![
        error_is_not_a_member_of_package::extract(input, &mut file_parse_cache),
        error_object_not_found::extract(input),
        error_symbol_is_missing_from_classpath::extract(input),
        error_symbol_type_missing_from_classpath::extract(input),
        error_type_not_found::extract(input),
        error_value_not_found::extract(input),
    ]
    .into_iter()
    .flat_map(|e| e.into_iter().flat_map(|inner| inner.into_iter()))
    .flat_map(|e| {
        let cached_file_data = file_parse_cache.load_file(&e.src_file_name);

        match cached_file_data {
            None => vec![e],
            Some(file_data) => {
                let extra_wildcard_imports: Vec<ScalaClassImportRequest> = file_data
                    .imports
                    .iter()
                    .filter_map(|e| match e.suffix {
                        crate::source_dependencies::SelectorType::SelectorList(_) => None,
                        crate::source_dependencies::SelectorType::WildcardSelector => {
                            Some(&e.prefix_section)
                        }
                        crate::source_dependencies::SelectorType::NoSelector => None,
                    })
                    .map(|prefix| ScalaClassImportRequest {
                        class_name: format!("{}.{}", prefix, e.class_name),
                        ..e.clone()
                    })
                    .collect();

                extra_wildcard_imports
                    .into_iter()
                    .chain(vec![e].into_iter())
                    .collect()
            }
        }
        .into_iter()
        .map(|o| o.to_class_import_request())
    })
    .collect();

    combined_vec
}

pub fn extract_suffix_errors(_input: &str) -> Vec<super::ClassSuffixMatch> {
    Vec::new()
}
