use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub mod web;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Schema {
    pub columns: Vec<SchemaColumn>,
}

impl Schema {
    /// Checks if the schema is valid, i.e. all the column names are unique.
    pub fn is_valid(&self) -> bool {
        let mut names = HashSet::<&str>::new();
        for col in &self.columns {
            if !names.insert(&col.name) {
                return false;
            }
        }

        true
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SchemaColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: SchemaColumnKind,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
#[serde(rename_all = "lowercase")]
pub enum SchemaColumnKind {
    Boolean,
    Int,
    Double,
    String,
}

impl SchemaColumnKind {
    pub fn get_sql_text(&self) -> &'static str {
        match self {
            SchemaColumnKind::Boolean => "BOOLEAN",
            SchemaColumnKind::Int => "INTEGER",
            SchemaColumnKind::Double => "REAL",
            SchemaColumnKind::String => "TEXT",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const NO_COLUMNS_PAYLOAD: &str = r#"{ "columns": [] }"#;

    pub const VALID_POST_PAYLOAD: &str = r#"{
    "columns": [
        {
            "name": "A",
            "type": "boolean"
        },
        {
            "name": "B",
            "type": "int"
        },
        {
            "name": "C",
            "type": "double"
        },
        {
            "name": "D",
            "type": "string"
        }
    ]
}"#;

    #[test]
    fn no_columns_deserializes() {
        let _: Schema = serde_json::from_str(NO_COLUMNS_PAYLOAD).unwrap();
    }

    #[test]
    fn it_deserializes() {
        let _: Schema = serde_json::from_str(VALID_POST_PAYLOAD).unwrap();
    }
}
