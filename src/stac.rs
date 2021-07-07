use chrono::{DateTime, Utc};
use serde::{Serialize};
use serde_json::{Map, Value, to_value};

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
  _ServiceDesc,
  _ServiceDoc,
  Parent,
  Child,
  Item
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
  _Feature
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CollectionType {
  Collection
}


pub trait ToStacLink {
  fn to_stac_link(&self, collection_url: &url::Url) -> StacLink;
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

impl ToStacLink for geojson::Feature {
  fn to_stac_link(&self, collection_url: &url::Url) -> StacLink {
    let id: String = match self.id.as_ref().unwrap() {
        geojson::feature::Id::String(s) => s.to_string(),
        geojson::feature::Id::Number(n) => n.to_string(),
    };

    StacLink {
      rel: StacRel::Item,
      media_type: String::from("application/geo+json"),
      href: collection_url.join(&id).unwrap().to_string()
    }
  }
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
      let conforms_to: Vec<String> = vec![
        String::from(STAC_CORE_DEF)
      ];

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
  pub datetime: DateTime<Utc>,
  pub created: Option<DateTime<Utc>>,
  pub updated: Option<DateTime<Utc>>,

  // non-standard properties
  pub spatial_resolution: Option<f64>
}

impl ItemProperties {
    pub fn to_map(&self) -> Map<String, Value> {
      let mut properties = Map::new();

      // This is a silly way to create a properties map...
      // Find a better way to convert to a format that fits in Feature.properties
      properties.insert(String::from("title"), to_value(&self.title).unwrap());
      properties.insert(String::from("description"), to_value(&self.description).unwrap());
      properties.insert(String::from("datetime"), to_value(&self.datetime).unwrap());
      properties.insert(String::from("created"), to_value(&self.created).unwrap());
      properties.insert(String::from("updated"), to_value(&self.updated).unwrap());
      properties.insert(String::from("spatial_resolution"), to_value(&self.spatial_resolution).unwrap());
      properties
    }
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
        id,
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
