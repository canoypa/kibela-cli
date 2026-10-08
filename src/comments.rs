use serde_json::{Value, json};

use crate::api::{Client, Error};

// Replies are nested inside each page of comments, so both page sizes multiply into the
// cost of one request, which Kibela caps at 10,000.
const PAGE: u32 = 20;
const REPLY_PAGE: u32 = 100;

const COMMENT_FIELDS: &str = "author { account realName } content createdAt";
const SELECTION_FIELDS: &str = "noteTextSelection { content startLineInMarkdown endLineInMarkdown \
     startColumnInMarkdown endColumnInMarkdown startInText endInText } isResolved";

pub fn fetch(client: &Client, path: &str) -> Result<Option<Value>, Error> {
    let Some(comments) = fetch_connection(client, path, "comments", COMMENT_FIELDS)? else {
        return Ok(None);
    };
    let inline_fields = format!("{COMMENT_FIELDS} {SELECTION_FIELDS}");
    let Some(inline_comments) = fetch_connection(client, path, "inlineComments", &inline_fields)?
    else {
        return Ok(None);
    };
    Ok(Some(
        json!({ "comments": comments, "inlineComments": inline_comments }),
    ))
}

fn fetch_connection(
    client: &Client,
    path: &str,
    field: &str,
    fields: &str,
) -> Result<Option<Value>, Error> {
    let query = format!(
        "query($path: String!, $after: String) {{ noteFromPath(path: $path) {{ \
           {field}(first: {PAGE}, after: $after) {{ totalCount pageInfo {{ hasNextPage endCursor }} \
             nodes {{ id {fields} replies(first: {PAGE}) {{ totalCount pageInfo {{ hasNextPage endCursor }} \
               nodes {{ {COMMENT_FIELDS} }} }} }} }} }} }}"
    );
    let mut total;
    let mut nodes = Vec::new();
    let mut after = Value::Null;
    loop {
        let data = match client.query(&query, json!({ "path": path, "after": after })) {
            Ok(data) => data,
            Err(Error::NotFound) => return Ok(None),
            Err(e) => return Err(e),
        };
        if data["noteFromPath"].is_null() {
            return Ok(None);
        }
        let connection = &data["noteFromPath"][field];
        total = connection["totalCount"].clone();
        nodes.extend(connection["nodes"].as_array().cloned().unwrap_or_default());
        let page = &connection["pageInfo"];
        if page["hasNextPage"] != true {
            break;
        }
        after = next_cursor(page)?;
    }

    for node in &mut nodes {
        if node["replies"]["pageInfo"]["hasNextPage"] == true {
            let rest = fetch_remaining_replies(
                client,
                &node["id"],
                next_cursor(&node["replies"]["pageInfo"])?,
            )?;
            if let Some(replies) = node["replies"]["nodes"].as_array_mut() {
                replies.extend(rest);
            }
        }
        if let Some(node) = node.as_object_mut() {
            node.shift_remove("id");
        }
        if let Some(replies) = node["replies"].as_object_mut() {
            replies.shift_remove("pageInfo");
        }
    }
    Ok(Some(json!({ "totalCount": total, "nodes": nodes })))
}

fn fetch_remaining_replies(
    client: &Client,
    id: &Value,
    mut after: Value,
) -> Result<Vec<Value>, Error> {
    let query = format!(
        "query($id: ID!, $after: String) {{ comment(id: $id) {{ \
           replies(first: {REPLY_PAGE}, after: $after) {{ pageInfo {{ hasNextPage endCursor }} \
             nodes {{ {COMMENT_FIELDS} }} }} }} }}"
    );
    let mut replies = Vec::new();
    loop {
        let data = client
            .query(&query, json!({ "id": id, "after": after }))
            .map_err(|e| match e {
                Error::NotFound => Error::Other(format!(
                    "comment {} was not found",
                    id.as_str().unwrap_or_default()
                )),
                e => e,
            })?;
        let connection = &data["comment"]["replies"];
        replies.extend(connection["nodes"].as_array().cloned().unwrap_or_default());
        if connection["pageInfo"]["hasNextPage"] != true {
            return Ok(replies);
        }
        after = next_cursor(&connection["pageInfo"])?;
    }
}

fn next_cursor(page_info: &Value) -> Result<Value, Error> {
    match &page_info["endCursor"] {
        Value::String(cursor) => Ok(Value::String(cursor.clone())),
        _ => Err(Error::Other(
            "the API reported a next page without a cursor".into(),
        )),
    }
}
