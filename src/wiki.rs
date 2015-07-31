use std::io::Read;

use hyper::Client;
use hyper::header::Connection;
use rustc_serialize::json::Json;
use url::percent_encoding;

use ::json;
use ::json::JsonPathElement::{Key, Only};

pub struct Wiki {
    hostname: String,
}

// TODO: return Revisions instead of IDs more places, where appropriate
pub struct Revision {
    pub revid: u64,
    pub parentid: u64,
    pub comment: String,
}

impl Wiki {
    /// Constructs a Wiki object representing the wiki at `hostname` (e.g. "en.wikipedia.org").
    pub fn new(hostname: &str) -> Wiki {
        Wiki { hostname: hostname.to_string() }
    }

    // TODO: return a Result
    /// Calls the MediaWiki API with the given parameters and format=json. Returns the raw JSON.
    fn call_mediawiki_api(&self, parameters: Vec<(&str, &str)>) -> String {
        let post_body =
            parameters.into_iter().map(|p| format!("{}={}", p.0, p.1))
            .collect::<Vec<_>>().join("&") + "&format=json";

        let client = Client::new();
        let mut response = client.post(&format!("https://{}/w/api.php", self.hostname))
            .body(&post_body)
            .header(Connection::close())
            .send().unwrap();

        let mut body = String::new();
        response.read_to_string(&mut body).unwrap();
        body
    }

    /// Returns the last `limit` revisions for the page `title`.
    pub fn get_revision_ids(&self, title: &str, limit: u64) -> Result<Vec<Revision>, String> {
        let json_str = self.call_mediawiki_api(
            vec![("action", "query"), ("prop", "revisions"), ("titles", title),
                 ("rvprop", "comment|ids"), ("rvlimit", &limit.to_string())]);
        let json = Json::from_str(&json_str).unwrap();
        let revisions = try!(
            json::get_json_array(&json, &[Key("query"), Key("pages"), Only, Key("revisions")]));

        // Check that  all revisions have revid and parentid (they all should).
        // TODO: what about making a Vec<Result<Revision>, String> and then rolling that up with a fold()? Would eliminate the repetition of the get_json_* calls.
        if revisions.into_iter().any(|revision| {
            json::get_json_number(revision, &[Key("revid")]).is_err() ||
            json::get_json_number(revision, &[Key("parentid")]).is_err() ||
            json::get_json_string(revision, &[Key("comment")]).is_err()
        }) {
            Err(format!(
                "One or more revisions of page \"{}\" have missing or invalid field", title))
        } else {
            Ok(revisions.into_iter().map(|revision| {
                Revision {
                    revid: json::get_json_number(revision, &[Key("revid")]).unwrap(),
                    parentid: json::get_json_number(revision, &[Key("parentid")]).unwrap(),
                    comment: json::get_json_string(revision, &[Key("comment")]).unwrap().to_string()
                }
            }).collect())
        }
    }

    /// Returns the latest revision ID for the page `title`.
    pub fn get_latest_revision_id(&self, title: &str) -> Result<u64, String> {
        // TODO: Can this be a one-liner? Does try!() work properly like that?
        let revisions = try!(self.get_revision_ids(title, 1));
        Ok(revisions[0].revid)
    }

    /// Returns the contents of the page `title` as of (i.e., immediately after) revision `id`.
    pub fn get_revision_content(&self, title: &str, id: u64) -> Result<String, String> {
        let json_str = self.call_mediawiki_api(
            vec![("action", "query"), ("prop", "revisions"), ("titles", title), ("rvprop", "content"),
                 ("rvlimit", "1"), ("rvstartid", &id.to_string())]);
        let json = Json::from_str(&json_str).unwrap();
        match json::get_json_string(
            &json, &[Key("query"), Key("pages"), Only, Key("revisions"), Only, Key("*")]) {
            Ok(content) => Ok(content.to_string()),
            Err(msg) => Err(msg),
        }
    }

    /// Follows all redirects to find the canonical name of the page at `title`.
    pub fn get_canonical_title(&self, title: &str) -> Result<String, String> {
        let latest_revision_id = self.get_latest_revision_id(title).unwrap();
        let page_contents = self.get_revision_content(title, latest_revision_id).unwrap();

        let regex = regex!(r"#REDIRECT \[\[([^]]+)\]\].*");
        match regex.captures(&page_contents) {
            Some(captures) => self.get_canonical_title(captures.at(1).unwrap()),
            None => {
                println!("Canonical page title is \"{}\"", title);
                Ok(title.to_string())
            },
        }
    }

    /// Parses the wikitext in `wikitext` as though it were the contents of the page `title`,
    /// returning the rendered HTML.
    pub fn parse_wikitext(&self, title: &str, wikitext: &str) -> Result<String, String> {
        let encoded_wikitext =
            percent_encoding::percent_encode(wikitext.as_bytes(), percent_encoding::QUERY_ENCODE_SET);
        let html = self.call_mediawiki_api(
            vec![("action", "parse"), ("prop", "text"), ("disablepp", ""), ("contentmodel", "wikitext"),
                 ("title", title), ("text", &encoded_wikitext)]);
        // TODO: check return value
        let json = Json::from_str(&html).unwrap();
        match json::get_json_string(&json, &[Key("parse"), Key("text"), Key("*")]) {
            Ok(contents) => Ok(contents.to_string()),
            Err(message) => Err(message),
        }
    }

    /// Gets the current, fully-rendered (**HTML**) contents of the page `title`.
    pub fn get_current_page_content(title: &str) -> String {
        // TODO: should this struct be keeping a Client instead of creating new ones for each method
        // call, here and in call_mediawiki_api?
        let client = Client::new();
        let mut res = client.get(
            &format!("https://en.wikipedia.org/wiki/{}", title))
            .header(Connection::close())
            .send().unwrap();

        let mut body = String::new();
        res.read_to_string(&mut body).unwrap();

        body
    }
}