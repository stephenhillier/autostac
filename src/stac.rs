use serde::{Serialize, Deserialize};

/// STAC Link relations help describe how each link relates to the current page.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum StacRel {
  /// The current page.
  /// named SelfRel here because Self is a reserved word.
  #[serde(rename = "self")]
  SelfRel,
  /// The root, or landing page, of the STAC API. 
  Root,
  ServiceDesc,
  ServiceDoc,
  Child
}

/// StacLink objects are used in the `links` field list.
#[derive(Serialize, Deserialize)]
struct StacLink {
  /// The relation to the current page. See StacRel
  rel: StacRel,
  /// The media type that the client can expect to be returned by the link
  /// TODO: this could probably be narrowed down more than "any string".
  #[serde(rename = "type")]
  media_type: String,
  /// A hyperlink
  href: String
}

/// A STAC landing page.
/// conforms to v1.0.0-beta2
#[derive(Serialize, Deserialize)]
pub struct LandingPage {
  stac_version: String,
  id: String,
  title: String,
  description: String,
  conforms_to: Vec<String>,
  links: Vec<StacLink>
}

impl LandingPage {
    pub fn new(id: String, title: String, description: String, base_path: String) -> LandingPage {
      let stac_version = "1.0.0";

      // form the "conforms_to" field.
      // this will have to be updated soon to allow new definitions that the service
      // conforms to.  For now, add the "core" definition (v1.0.0-beta.2).
      let stac_core_def = "https://api.stacspec.org/v1.0.0-beta.2/core";
      let mut conforms_to: Vec<String> = Vec::new();
      conforms_to.push(String::from(stac_core_def));

      // Add root and self links to a list of links.
      // again, this will have to support catalog links.
      let mut links: Vec<StacLink> = Vec::new();

      let root_link = StacLink {
        rel: StacRel::Root,
        media_type: String::from("application/json"),
        href: base_path.to_owned()
      };

      let self_link = StacLink {
        rel: StacRel::SelfRel,
        media_type: String::from("application/json"),
        href: base_path.to_owned()
      };

      links.push(root_link);
      links.push(self_link);

      LandingPage {
        stac_version: String::from(stac_version),
        id,
        title,
        description,
        conforms_to,
        links
      }
    }
}
