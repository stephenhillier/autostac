use serde::{Serialize};

/// this STAC implementation was written against the v1.0.0-beta2 version of the
/// STAC spec.
static STAC_VERSION: &str = "1.0.0";
static STAC_CORE_DEF: &str = "https://api.stacspec.org/v1.0.0-beta.2/core";

/// STAC Link relations help describe how each link relates to the current page.
#[derive(Serialize)]
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
#[derive(Serialize)]
pub struct StacLink {
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
#[derive(Serialize)]
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
      // form the "conforms_to" field.
      // this will have to be updated soon to allow new definitions that the service
      // conforms to.  For now, add the "core" definition (v1.0.0-beta.2).
      let mut conforms_to: Vec<String> = Vec::new();
      conforms_to.push(String::from(STAC_CORE_DEF));

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
        stac_version: String::from(STAC_VERSION),
        id,
        title,
        description,
        conforms_to,
        links
      }
    }
}

/// A STAC Item.
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/item-spec/item-spec.md

#[derive(Serialize)]
pub struct Item {

}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CatalogType {
  Catalog
}

/// A STAC Catalog.
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/catalog-spec/catalog-spec.md
#[derive(Serialize)]
pub struct Catalog {
  #[serde(rename = "type")]
  pub catalog_type: CatalogType,
  pub stac_version: String,
  pub id: String,
  pub title: String,
  pub description: String,

  /// TODO:  this might not need to be populated when the catalog is created,
  /// instead, this field should be created at serialization time
  /// by a custom serializer for the items field.
  pub links: Vec<StacLink>,
  
  /// the items within the catalog.
  #[serde(skip)]
  items: Vec<Item>
}

impl Catalog {
    /// create a new Catalog containing a list of Items
    pub fn new(
      id: String,
      title: String,
      description: String,
      path: String,
      items: Vec<Item>,
    ) -> Catalog {
      // Add root and self links to a list of links.
      let mut links: Vec<StacLink> = Vec::new();

      let root_link = StacLink {
        rel: StacRel::Root,
        media_type: String::from("application/json"),
        href: path.to_owned()
      };

      let self_link = StacLink {
        rel: StacRel::SelfRel,
        media_type: String::from("application/json"),
        href: path.to_owned()
      };
      
      links.push(root_link);
      links.push(self_link);

      Catalog {
        catalog_type: CatalogType::Catalog,
        stac_version: String::from(STAC_VERSION),
        id,
        title,
        description,
        items,
        links
      }
    }
}
