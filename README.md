# anchor_test
My implementation of the backend homework task sent by Anchor.

## Usage
Make sure you have [Rust installed](https://rustup.rs/) (tested with Rust 1.74 or later). Run the following command:
```
$ cargo run --release
```
The server will bind to localhost:8080 - using port 8080 instead of 80 for convenience (since it's privileged).

## Testing
Simply run:
```
$ cargo test
```
Which will check all of the unit and integration tests.

## Architecture
The server implements 3 endpoints, as required:
- `POST /sheet` - create a new sheet using the provided schema.
    The request body (i.e. the sheet schema) must be a JSON object with the following format:
    ```json5
    {
        "columns": [
            {
                "name": "<column name>",
                "type": "<column type>"
            },
            // ... (repeated as many times as needed)
        ]
    }
    ```
    Column names must be unique, and must not contain double quotes (`"`).  
    Column type must be one of `boolean`, `int`,`double` or `string`.

    The response body will be a JSON object. Successful responses will have the format:
    ```json5
    {
        "sheet_id": "<generated sheet id>"
    }
    ```
    Otherwise, error responses will have the format:
    ```json5
    {
        "error": "<explanation>"
    }
    ```

- `POST /sheet/:sheetid` - set a specific cell's value within the specified sheet.
    The request body must be a JSON object with the following format:
    ```json5
    {
        "column": "<column name>",
        "row": /* <row number> */,
        "value": /* <value> */
    }
    ```
    `column` must be a name belonging to an existing column within the sheet.  
    `row` must be an integer.  
    `value` must be a valid value according to the column's type, OR a string of the form `"lookup(\"<column name>\",<row number>)"` (more specifically, matching the regex `^lookup\(\s*"([^"]+)"\s*,\s*(\d+)\s*\)$`) where the column name is a valid name in the same sheet.

    Lookup cells cannot form cycles - attempting to do so will fail with an error.

- `GET /sheet/:sheetid` - get the content of the entire sheet with the given id.
    The response body will be a JSON object with the following format:
    ```json5
    {
        "columns": {
            "<column name>": [
                {
                    "row": /* <cell row> */,
                    "value": /* <cell value> */
                },
                // ... (one entry for each populated cell in the column)
            ],
            // ... (one entry for each column)
        }
    }
    ```
    Lookup cells which point to a nonexistent value will be returned as having a `null` value (and this is the only case where `null` will appear as a value). This behavior is configurable - set the environment variable `NO_LOOKUP_NULLS` to remove these cells from the output entirely.