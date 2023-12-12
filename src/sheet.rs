use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use regex::Regex;
use serde::{Deserialize, Serialize};

pub mod web;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Schema {
    pub columns: Vec<SchemaColumn>,
}

impl Schema {
    /// Checks if the schema is valid, i.e. all the column names are unique
    /// and none of them contain double quotes.
    pub fn is_valid(&self) -> bool {
        let mut names = HashSet::<&str>::new();
        for col in &self.columns {
            if col.name.contains('"') || !names.insert(&col.name) {
                return false;
            }
        }

        true
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SchemaColumn {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: SchemaColumnKind,
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
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
            Self::Boolean => "BOOLEAN",
            Self::Int => "INTEGER",
            Self::Double => "REAL",
            Self::String => "TEXT",
        }
    }

    pub fn from_sql_text(text: &str) -> Option<Self> {
        match text {
            "BOOLEAN" => Some(Self::Boolean),
            "INTEGER" => Some(Self::Int),
            "REAL" => Some(Self::Double),
            "TEXT" => Some(Self::String),
            _ => None,
        }
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Cell {
    pub column: String,
    pub row: i64,
    pub value: CellValue,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum CellValue {
    Boolean(bool),
    Int(i64),
    Double(f64),
    String(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LookupCellValue {
    pub target_col: String,
    pub target_row: i64,
}

static LOOKUP_REGEX: OnceLock<Regex> = OnceLock::new();

impl CellValue {
    pub fn is_lookup(&self) -> Option<LookupCellValue> {
        let Self::String(s) = &self else {
            return None;
        };

        let re = LOOKUP_REGEX
            .get_or_init(|| Regex::new(r#"^lookup\(\s*"([^"]+)"\s*,\s*(\d+)\s*\)$"#).unwrap());

        let Some((_, [col_name, row])) = re.captures(s).map(|c| c.extract()) else {
            return None;
        };

        let Ok(row) = row.parse() else {
            return None;
        };

        Some(LookupCellValue {
            target_col: col_name.into(),
            target_row: row,
        })
    }
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SheetContent {
    pub columns: HashMap<String, Vec<SheetContentColumn>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SheetContentColumn {
    pub row: i64,
    pub value: Option<CellValue>,
}

#[cfg(test)]
impl SheetContent {
    pub fn build_with_triples(triples: &[(&str, i64, Option<CellValue>)]) -> Self {
        let mut columns = HashMap::new();

        for (column, row, value) in triples {
            let column: &mut Vec<SheetContentColumn> =
                columns.entry(column.to_string()).or_default();
            column.push(SheetContentColumn {
                row: *row,
                value: value.clone(),
            })
        }

        Self { columns }
    }

    pub fn with_potential_empty_columns(mut self, cols: &[&str]) -> Self {
        for col in cols {
            self.columns.entry(col.to_string()).or_default();
        }

        self
    }

    pub fn with_sorted_columns(mut self) -> Self {
        for col in self.columns.values_mut() {
            col.sort_unstable_by_key(|x| x.row);
        }

        self
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
            "name": "B2",
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
        let schema: Schema = serde_json::from_str(VALID_POST_PAYLOAD).unwrap();
        assert_eq!(
            schema,
            Schema {
                columns: vec![
                    SchemaColumn {
                        name: "A".into(),
                        kind: SchemaColumnKind::Boolean
                    },
                    SchemaColumn {
                        name: "B".into(),
                        kind: SchemaColumnKind::Int
                    },
                    SchemaColumn {
                        name: "B2".into(),
                        kind: SchemaColumnKind::Int
                    },
                    SchemaColumn {
                        name: "C".into(),
                        kind: SchemaColumnKind::Double
                    },
                    SchemaColumn {
                        name: "D".into(),
                        kind: SchemaColumnKind::String
                    }
                ]
            }
        );
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

    #[test]
    fn valid_schema() {
        let schema: Schema = serde_json::from_str(VALID_POST_PAYLOAD).unwrap();
        assert!(schema.is_valid());
    }

    #[test]
    fn invalid_schema_duplicate() {
        let mut schema: Schema = serde_json::from_str(VALID_POST_PAYLOAD).unwrap();
        schema.columns[1].name = "A".into();
        assert!(!schema.is_valid());
    }

    #[test]
    fn invalid_schema_quotes() {
        let mut schema: Schema = serde_json::from_str(VALID_POST_PAYLOAD).unwrap();
        schema.columns[0].name = r#""quotes""#.into();
        assert!(!schema.is_valid());
    }

    #[test]
    fn valid_lookup() {
        let val = CellValue::String(r#"lookup("hello", 5)"#.into());
        assert_eq!(
            val.is_lookup(),
            Some(LookupCellValue {
                target_col: "hello".into(),
                target_row: 5
            })
        )
    }

    #[test]
    fn invalid_lookup() {
        let val = CellValue::String(r#"yo"#.into());
        assert!(val.is_lookup().is_none())
    }
}
