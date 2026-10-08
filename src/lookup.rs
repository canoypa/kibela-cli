use serde_json::{Value, json};

use crate::api::{Client, Error, next_cursor};

const GROUP_PAGE: u32 = 100;

/// All groups, archived ones included, as `{"totalCount": …, "nodes": [{ id name isArchived }]}`.
/// The API returns archived groups only from `archivedGroups`.
pub fn groups(client: &Client) -> Result<Value, Error> {
    let (active_total, mut nodes) = connection(client, "groups")?;
    let (archived_total, archived) = connection(client, "archivedGroups")?;
    nodes.extend(archived);
    let total = active_total + archived_total;
    Ok(json!({ "totalCount": total, "nodes": nodes }))
}

fn connection(client: &Client, field: &str) -> Result<(u64, Vec<Value>), Error> {
    let query = format!(
        "query($after: String) {{ {field}(first: {GROUP_PAGE}, after: $after) {{ \
           totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ id name isArchived }} }} }}"
    );
    let mut nodes = Vec::new();
    let mut after = Value::Null;
    loop {
        let data = client.query(&query, json!({ "after": after }))?;
        let connection = &data[field];
        nodes.extend(connection["nodes"].as_array().cloned().unwrap_or_default());
        let page = &connection["pageInfo"];
        if page["hasNextPage"] != true {
            return Ok((connection["totalCount"].as_u64().unwrap_or_default(), nodes));
        }
        after = next_cursor(page)?;
    }
}

pub fn group_id(groups: &Value, name: &str) -> Result<Value, String> {
    let nodes = groups["nodes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let matches: Vec<&Value> = nodes.iter().filter(|g| g["name"] == name).collect();
    match matches.as_slice() {
        [group] => Ok(group["id"].clone()),
        [] => Err(format!(
            "group {name} was not found. Run `kibela group list`"
        )),
        _ => Err(format!("{} groups are named {name}", matches.len())),
    }
}

pub fn folder_id(client: &Client, path: &str) -> Result<Option<Value>, Error> {
    let data = match client.query(
        "query($path: String!) { folderFromPath(path: $path) { id } }",
        json!({ "path": path }),
    ) {
        Ok(data) => data,
        Err(Error::NotFound) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(Some(data["folderFromPath"]["id"].clone()).filter(|id| !id.is_null()))
}

pub fn user_id(client: &Client, account: &str) -> Result<Option<Value>, Error> {
    let data = match client.query(
        "query($account: String!) { userFromAccount(account: $account) { id } }",
        json!({ "account": account }),
    ) {
        Ok(data) => data,
        Err(Error::NotFound) => return Ok(None),
        Err(e) => return Err(e),
    };
    Ok(Some(data["userFromAccount"]["id"].clone()).filter(|id| !id.is_null()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_names_match_exactly() {
        let groups = json!({ "totalCount": 4, "nodes": [
            { "id": "G1", "name": "Design" },
            { "id": "G2", "name": "Design review" },
            { "id": "G3", "name": "Twin" },
            { "id": "G4", "name": "Twin" },
        ] });
        assert_eq!(group_id(&groups, "Design"), Ok(json!("G1")));
        assert!(group_id(&groups, "design").is_err());
        assert!(group_id(&groups, "Twin").is_err());
    }
}
