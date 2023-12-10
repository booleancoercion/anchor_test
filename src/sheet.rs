use std::{collections::HashSet, fmt::Display};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
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

#[derive(Serialize, Deserialize)]
pub struct SchemaColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: SchemaColumnKind,
}

#[derive(Serialize, Deserialize, Copy, Clone)]
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
