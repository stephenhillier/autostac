use std::collections::HashMap;
use std::f64;
use std::path::PathBuf;
use std::fs;
use chrono::DateTime;
use chrono::offset::FixedOffset;
use geo::polygon;
use geo::algorithm::intersects::Intersects;
use gdal::{Dataset, Metadata};
use geo::prelude::BoundingRect;
use geojson::Feature;
use geojson::FeatureCollection;
use geojson::Geometry;
use geo_types::Polygon;
use serde_json::{Map, Value, to_value};
use serde::{Serialize};
use url;
use crate::stac;
use crate::transform;

/// Service represents the raster imagery service.
/// For v0.0.1, the idea is that imagery of various sources can be filtered, or automatically
/// chosen based on resolution, least cloud coverage, or date ranges. Rasters represent
/// more varied thematic data like digital elevation models, or derived products (hillshade).
/// These files can be catalogued by the TIFFTAG_IMAGEDESCRIPTION GeoTIFF tag, and users can
/// make queries like "what resolution of DEM coverage is here".
#[derive(Debug)]
pub struct Service {
  pub id: String,
  pub title: String,
  pub description: String,
  pub base_url: url::Url,
  pub collections: HashMap<String, ImageryCollection>
}

impl Service {
    pub fn stac_landing(&self) -> stac::LandingPage {
      stac::LandingPage::new(
        self.id.to_owned(),
        self.title.to_owned(),
        self.description.to_owned(),
        &self.base_url,
        self.collections.as_stac_collections_vec(
          &self.base_url.join("collections/").unwrap()
        )
      )
    }
}

/// Convert a list of imagery metadata into a GeoJSON FeatureCollection
pub trait AsFeatureCollection {
  /// converts a collection of files into a GeoJSON FeatureCollection
  fn as_feature_collection(self) -> FeatureCollection;
}

/// ImageryCollection stores metadata about spectral imagery files such as
/// satellite imagery.
#[derive(Debug)]
pub struct ImageryCollection {
  pub id: String,
  title: String,
  description: String,
  files: Vec<ImageryFile>
}

impl ImageryCollection {
  /// Create a new ImageryCollection, populated with files found by
  /// collect_files.
  pub fn new(id: String, title: String, description: String) -> ImageryCollection {
    let files = ImageryCollection::collect_files();
    ImageryCollection{
      id,
      title,
      description,
      files
    }
  }

  /// register_images searches the imagery directory and collects
  /// metadata about valid images.  Images are valid if they can be
  /// opened by GDAL.
  fn collect_files() -> Vec<ImageryFile> {
    // just a hardcoded dir path for now.  Put satellite imagery (e.g. Sentinel-2)
    // into this folder.
    let img_dir = fs::read_dir("./data/imagery/").unwrap();

    let mut coverage: Vec<ImageryFile> = Vec::new();

    // iterate through the files in img_dir and capture information
    for file in img_dir {
      let file = file.unwrap();
        let path = file.path();
        let filename = file.path().as_path().file_stem().unwrap().to_str().unwrap().to_owned();
        println!("{}", filename);

        // open the dataset using GDAL.
        let dataset = Dataset::open(&path).unwrap();
        let poly = get_extent(&dataset);
        let crs = dataset.projection();
        let num_bands = dataset.raster_count() as u16;
        
        // Check metadata for cloud coverage
        // this is the metadata key for Sentinel-2 imagery.
        // todo: confirm key for other sources.
        let cloud_coverage: Option<f64> = dataset
            .metadata_item("CLOUD_COVERAGE_ASSESSMENT", "")
            .map(|s| s.parse::<f64>().unwrap());

        // Check metadata for timestamp. "PRODUCT_START_TIME" is used, but
        // need to confirm whether this is the most appropriate timestamp.
        let timestamp: Option<DateTime<FixedOffset>> = dataset
            .metadata_item("PRODUCT_START_TIME", "")
            .map(|s| DateTime::parse_from_rfc3339(&s).unwrap());


        // capture the IMAGEDESCRIPTION tag. We can allow users to
        // set this tag as a basic way to group images. e.g. "DEM",
        // "Stream_Burned_DEM". The tag value is up to the user.
        let description: Option<String> = dataset
            .metadata_item("TIFFTAG_IMAGEDESCRIPTION", "");

        // convert extent polygon into lat/long
        let boundary: Polygon<f64> = transform::transform_polygon(&poly, &crs, "EPSG:4326");

        // add the file information to the coverage vector.
        let properties = ImageryFileProperties {
            filename: path.as_path().display().to_string(),
            crs,
            resolution: get_resolution_from_geotransform(&dataset.geo_transform().unwrap()),
            description,
            num_bands,
            cloud_coverage,
            timestamp,
            red_band: None, // unimplemented
            ni_band: None  // unimplemented
        };

        let file = ImageryFile{
            path,
            filename,
            boundary,
            properties
        };
        coverage.push(file);
    }
    coverage
  }

  pub fn stac_collection(
    &self,
    base_url: &url::Url
  ) -> stac::Collection {
    let collection_url = base_url
      .join("collections/").unwrap()
      .join(&(self.id.to_owned() + "/")).unwrap();

    let mut collection = stac::Collection::new(
      self.id.to_owned(),
      self.title.to_owned(),
      self.description.to_owned(),
      Vec::new(),
    );

    collection.links.push(collection.root_link(base_url));
    collection.links.push(collection.self_link(base_url));

    for f in self.all() {
      let item = f.to_stac_item();
      collection.links.push(item.item_link(&collection_url));
    }

    collection
  }

  /// returns all the files in ImageryCollection.
  pub fn all(&self) -> &Vec<ImageryFile> {
    &self.files
  }

  /// Returns files in ImageryCollection that intersect with bounds (EPSG:3857)
  pub fn intersects(&self, bounds: &Polygon<f64>) -> Vec<ImageryFile> {
    let mut matching_files: Vec<ImageryFile> = Vec::new();
    for f in self.files.iter() {
        if f.boundary.intersects(bounds) {
            matching_files.push(f.to_owned());
        }
    };
    matching_files
  }

  /// get an item by its ID.
  pub fn get_item(&self, item_id: String) -> Option<&ImageryFile> {
    for f in self.files.iter() {
      if f.filename == item_id {
        return Some(f)
      }
    }
    None
  }
}

impl AsFeatureCollection for &Vec<ImageryFile> {
  /// converts a vec of ImageryFiles into a FeatureCollection
  fn as_feature_collection(self) -> FeatureCollection {
    let mut fc = FeatureCollection {
      bbox: None,
      features: vec![],
      foreign_members: None
    };
    for rast in self {
        let geometry = Geometry::from(&rast.boundary);
        let bbox_rect = rast.boundary.bounding_rect().unwrap();
        let bbox: Option<Vec<f64>> = Some(vec![
          bbox_rect.min().x,
          bbox_rect.min().y,
          bbox_rect.max().x,
          bbox_rect.max().y,
        ]);
        let properties = rast.properties.to_map();

        let feat = Feature {
            id: None,
            bbox,
            geometry: Some(geometry),
            properties: Some(properties),
            foreign_members: None
        };
        fc.features.push(feat);
    };
    fc
  }
}

trait AsSTACCollections {
  fn as_stac_collections_vec(&self, base_url: &url::Url) -> Vec<stac::Collection>;
}

impl AsSTACCollections for HashMap<String,ImageryCollection> {
  fn as_stac_collections_vec(&self, base_url: &url::Url) -> Vec<stac::Collection> {
    self.iter().map(|(_,v)| {
      println!("base_url {:?}", &base_url);
      v.stac_collection(base_url)
    }).collect()
  }
}

/// Resolution represents the horizontal (x) and vertical (y)
/// length of a single pixel, in the map units.
/// TODO:  this needs to be converted to m, not use the base map units.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Resolution {
    pub y: f64,
    pub x: f64
}

#[derive(Debug,Clone)]
pub struct ImageryFileProperties {
  pub filename: String,
  pub crs: String,
  pub resolution: Resolution,
  pub num_bands: u16,
  pub description: Option<String>,
  pub cloud_coverage: Option<f64>,
  pub timestamp: Option<DateTime<FixedOffset>>,
  pub red_band: Option<u16>,
  pub ni_band: Option<u16>
}

impl ImageryFileProperties {
  /// Converts ImageryFileProperties to a serde_json::Map so
  /// that it can be used as the properties object in a GeoJSON Feature 
  pub fn to_map(&self) -> Map<String, Value> {
      let mut properties = Map::new();

      // This is a silly way to create a properties map...
      // Find a better way to convert to a format that fits in Feature.properties
      properties.insert(String::from("filename"), to_value(&self.filename).unwrap());
      properties.insert(String::from("crs"), to_value(&self.crs).unwrap());
      properties.insert(String::from("resolution"), to_value(&self.resolution).unwrap());
      properties.insert(String::from("num_bands"), to_value(&self.num_bands).unwrap());
      properties.insert(String::from("cloud_coverage"), to_value(&self.cloud_coverage).unwrap());
      properties.insert(String::from("timestamp"), to_value(&self.timestamp).unwrap());
      properties.insert(String::from("red_band"), to_value(&self.red_band).unwrap());
      properties.insert(String::from("ni_band"), to_value(&self.ni_band).unwrap());
      properties.insert(String::from("description"), to_value(&self.description).unwrap());
      properties
  }
}

/// metadata about images
#[derive(Debug, Clone)]
pub struct ImageryFile {
  path: PathBuf,
  filename: String,
  pub boundary: Polygon<f64>,
  pub properties: ImageryFileProperties
}

impl ImageryFile {
    pub fn to_stac_item(&self) -> stac::Item {
      let properties = stac::ItemProperties {
        // todo: if datetime is required by STAC, make timestamp required on ImageryFile.
        // this would help avoid unwrap().
        datetime: self.properties.timestamp.unwrap(),
        title: self.properties.filename.to_owned(),
        description: self.properties.description.to_owned(),
        created: None, // unimplemented
        updated: None // unimplemented
      };
      let filename: String = self.filename.to_owned();
      stac::Item {
        item_type: stac::ItemType::Feature,
        properties,
        id: filename.to_owned(),
        geometry: geojson::Geometry::from(&self.boundary),
        bbox: Vec::new(), // unimplemented
        links: Vec::new(),
        assets: Vec::new(),
        collection: None,
        path: filename
      }
    }
}

/// get_resolution_from_geotransform uses a raster image's geotransform
/// to determine the resolution.
/// https://gdal.org/tutorials/geotransforms_tut.html
fn get_resolution_from_geotransform(geotransform: &[f64]) -> Resolution {
  let xwidth = (geotransform[1].powi(2) + geotransform[2].powi(2)).sqrt();
  let ywidth = (geotransform[5].powi(2) + geotransform[4].powi(2)).sqrt();
  Resolution{x: xwidth, y: ywidth}
}

/// get_extent calculates the extent of a given dataset and
/// returns a geo_types::Polygon representing it.
fn get_extent(dataset: &Dataset) -> Polygon<f64> {
  let [xmin, x_size, _, ymin, _, y_size] = dataset.geo_transform().unwrap();
  let (width, height) = dataset.raster_size();
  let xmax = xmin + width as f64 * x_size;
  let ymax = ymin + height as f64 * y_size;
  polygon![
      (x: xmin, y: ymin),
      (x: xmax, y: ymin),
      (x: xmax, y: ymax),
      (x: xmin, y: ymax)
  ]
}
