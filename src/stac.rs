use chrono::{DateTime, FixedOffset};
use serde::{Serialize};
use geojson;
use url;
use crate::catalog;

/// this STAC implementation was written against the v1.0.0-beta2 version of the
/// STAC spec.
/// The structs may contain additional fields (and methods) but the serialized representations
/// should only include fields conforming to the STAC spec
static STAC_VERSION: &str = "1.0.0";
static STAC_CORE_DEF: &str = "https://api.stacspec.org/v1.0.0-beta.2/core";

/// STAC Link relations help describe how each link relates to the current page.
#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StacRel {
  /// The current page.
  /// named SelfRel here because Self is a reserved word.
  #[serde(rename = "self")]
  SelfRel,
  /// The root, or landing page, of the STAC API. 
  Root,
  ServiceDesc,
  ServiceDoc,
  Parent,
  Child,
  Item
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
  Feature
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionType {
  Collection
}


/// StacLink objects are used in the `links` field list of STAC endpoints.
#[derive(Debug, Serialize)]
pub struct StacLink {
  /// The relation to the current page. See StacRel
  pub rel: StacRel,
  /// The media type that the client can expect to be returned by the link
  /// TODO: this could probably be narrowed down more than "any string".
  #[serde(rename = "type")]
  pub media_type: String,
  /// A hyperlink
  pub href: String
}

/// A STAC landing page.
/// conforms to v1.0.0-beta2
#[derive(Debug, Serialize)]
pub struct LandingPage {
  stac_version: String,
  id: String,
  title: String,
  description: String,
  conforms_to: Vec<String>,
  links: Vec<StacLink>
}

impl LandingPage {
    /// create and return a new STAC Landing Page
    pub fn new(id: String, title: String, description: String, base_url: &url::Url, collections: Vec<Collection>) -> LandingPage {
      // form the "conforms_to" field.
      // this will have to be updated soon to allow new definitions that the service
      // conforms to.  For now, add the "core" definition (v1.0.0-beta.2).
      let mut conforms_to: Vec<String> = Vec::new();
      conforms_to.push(String::from(STAC_CORE_DEF));

      // Add root and self links to a list of links.
      // again, this will have to support collection links.
      let mut links: Vec<StacLink> = Vec::new();

      let root_link = StacLink {
        rel: StacRel::Root,
        media_type: String::from("application/json"),
        href: base_url.to_string()
      };

      let self_link = StacLink {
        rel: StacRel::SelfRel,
        media_type: String::from("application/json"),
        href: base_url.to_string()
      };

      let mut collection_links = collections
        .into_iter()
        .map(|v| {
          v.collection_link(base_url)
        }).collect::<Vec<_>>();

      links.push(root_link);
      links.push(self_link);
      links.append(&mut collection_links);

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

/// Properties of a STAC Item.
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/item-spec/item-spec.md#properties-object
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/item-spec/common-metadata.md#stac-common-metadata
#[derive(Debug, Serialize)]
pub struct ItemProperties {
  pub title: String,
  pub description: Option<String>,
  pub datetime: DateTime<FixedOffset>,
  pub created: Option<DateTime<FixedOffset>>,
  pub updated: Option<DateTime<FixedOffset>>,
}

/// A STAC Item.
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/item-spec/item-spec.md
#[derive(Debug, Serialize)]
pub struct Item {
  #[serde(rename = "type")]
  pub item_type: ItemType,
  pub properties: ItemProperties,
  pub id: String,
  pub geometry: geojson::Geometry,
  pub bbox: Vec<f64>,
  pub links: Vec<StacLink>,
  pub assets: Vec<String>,
  pub collection: Option<String>,
  #[serde(skip)]
  pub path: String
}

impl Item {
    /// create an item link by combining `collection_url` and the item's ID.
    pub fn item_link(&self, collection_url: &url::Url) -> StacLink {
      StacLink {
        rel: StacRel::Item,
        media_type: String::from("application/geo+json"),
        href: collection_url.join(&(self.id.to_owned())).unwrap().to_string()
      }
    }

    /// create a link back to this item's collection, with a ref of parent
    pub fn parent_link(&self, collection_url: &url::Url) -> StacLink {
      StacLink {
        rel: StacRel::Parent,
        media_type: String::from("application/geo+json"),
        href: collection_url.to_string()
      }
    }
}

/// A STAC Collection.
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/collection-spec/collection-spec.md
#[derive(Debug, Serialize)]
pub struct Collection {
  #[serde(rename = "type")]
  collection_type: CollectionType,
  stac_version: String,
  pub id: String,
  pub title: String,
  pub description: String,
  pub links: Vec<StacLink>,
}

impl Collection {
    /// create a new Collection containing a list of Items
    pub fn new(
      id: String,
      title: String,
      description: String,
      links: Vec<StacLink>,
    ) -> Collection {
      Collection {
        collection_type: CollectionType::Collection,
        stac_version: String::from(STAC_VERSION),
        id: id.to_owned(),
        title,
        description,
        links
      }
    }

    /// return a link to this collection, with a ref of child
    pub fn collection_link(&self, base_url: &url::Url) -> StacLink {
      let collection_url = base_url
        .join("collections/").unwrap()
        .join(&(self.id.to_owned() + "/")).unwrap();
      println!("{}", collection_url.to_string());
      StacLink { rel: StacRel::Child,
        href: collection_url.to_string(),
        media_type: String::from("application/json")
        }
    }

    /// return a self-link for this collection
    pub fn self_link(&self, base_url: &url::Url) -> StacLink {
      let collection_url = base_url
        .join("collections/").unwrap()
        .join(&(self.id.to_owned() + "/")).unwrap();
      StacLink { rel: StacRel::SelfRel,
        href: collection_url.to_string(),
        media_type: String::from("application/json")
        }
    }

    /// return the root link for the service.
    /// this probably doesn't need to be a method of a collection;
    /// some refactoring is needed.
    pub fn root_link(&self, base_url: &url::Url) -> StacLink {
      StacLink { rel: StacRel::Root,
        href: base_url.to_string(),
        media_type: String::from("application/json")
        }
    }
}
