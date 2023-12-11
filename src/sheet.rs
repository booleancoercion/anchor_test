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

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Cell {
    pub column: String,
    pub row: i64,
    pub value: CellValue,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum CellValue {
    Boolean(bool),
    Int(i64),
    Double(f64),
    String(String),
}

impl From<&CellValue> for SchemaColumnKind {
    fn from(value: &CellValue) -> Self {
        match value {
            CellValue::Boolean(_) => Self::Boolean,
            CellValue::Double(_) => Self::Double,
            CellValue::Int(_) => Self::Int,
            CellValue::String(_) => Self::String,
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

    #[test]
    fn valid_cells_deserialize() {
        assert_eq!(
            serde_json::from_str::<Cell>(r#"{"column": "A", "row": 5, "value": true}"#).unwrap(),
            Cell {
                column: String::from("A"),
                row: 5,
                value: CellValue::Boolean(true)
            }
        );

        assert_eq!(
            serde_json::from_str::<Cell>(r#"{"column": "B", "row": -1, "value": 50}"#).unwrap(),
            Cell {
                column: String::from("B"),
                row: -1,
                value: CellValue::Int(50)
            }
        );

        assert_eq!(
            serde_json::from_str::<Cell>(r#"{"column": "C", "row": 0, "value": 5.0}"#).unwrap(),
            Cell {
                column: String::from("C"),
                row: 0,
                value: CellValue::Double(5.0)
            }
        );

        assert_eq!(
            serde_json::from_str::<Cell>(r#"{"column": "D", "row": 38291, "value": "string"}"#)
                .unwrap(),
            Cell {
                column: String::from("D"),
                row: 38291,
                value: CellValue::String("string".into())
            }
        );
    }
}
