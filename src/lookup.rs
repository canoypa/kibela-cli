use serde_json::{Value, json};

use crate::api::{Client, Error, next_cursor};
use crate::location::GroupKey;

const GROUP_PAGE: u32 = 100;

/// All groups, archived ones included, as
/// `{"totalCount": …, "nodes": [{ id name path isArchived }]}`.
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
           totalCount pageInfo {{ hasNextPage endCursor }} nodes {{ id name path isArchived }} }} }}"
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

pub fn group_id(groups: &Value, key: &GroupKey) -> Result<Value, String> {
    let (field, value) = match key {
        GroupKey::Path(path) => ("path", path),
        GroupKey::Name(name) => ("name", name),
    };
    let nodes = groups["nodes"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    let matches: Vec<&Value> = nodes.iter().filter(|g| g[field] == **value).collect();
    match matches.as_slice() {
        [group] => Ok(group["id"].clone()),
        [] => Err(format!(
            "group {value} was not found. Run `kibela group list`"
        )),
        _ => Err(format!(
            "{} groups are named {value}. Pass the path of one instead",
            matches.len()
        )),
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
            { "id": "G1", "name": "Design", "path": "/groups/1" },
            { "id": "G2", "name": "Design review", "path": "/groups/2" },
            { "id": "G3", "name": "Twin", "path": "/groups/3" },
            { "id": "G4", "name": "Twin", "path": "/groups/4" },
        ] });
        let name = |name: &str| GroupKey::Name(name.into());
        assert_eq!(group_id(&groups, &name("Design")), Ok(json!("G1")));
        assert!(group_id(&groups, &name("design")).is_err());
        assert!(group_id(&groups, &name("Twin")).is_err());
        let path = GroupKey::Path("/groups/4".into());
        assert_eq!(group_id(&groups, &path), Ok(json!("G4")));
    }
}
