use std::collections::HashMap;
use std::f64;
use std::path::Path;
use std::path::PathBuf;
use std::fs;
use chrono::{DateTime, Utc, TimeZone};
use geo::point;
use geo::prelude::HaversineDistance;
use http;
use geo::polygon;
use geo::algorithm::intersects::Intersects;
use geo::algorithm::contains::Contains;
use gdal::{Dataset, Metadata};
use geo::prelude::BoundingRect;
use geojson::Feature;
use geojson::FeatureCollection;
use geojson;
use geo_types::{Polygon, Geometry};
use s3;
use serde_json::Value;
use serde_json::to_value;
use serde_json::{Map};
use serde::{Serialize};
use url;
use crate::stac;
use crate::stac::ToStacLink;
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
  pub fn new_from_dir(id: String, title: String, description: String, dir: PathBuf) -> ImageryCollection {
    let files = ImageryCollection::collect_files(dir, &id);
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
  fn collect_files(dir: PathBuf, collection_id: &str) -> Vec<ImageryFile> {
    let img_dir = fs::read_dir(dir).unwrap();

    let mut coverage: Vec<ImageryFile> = Vec::new();

    // iterate through the files in img_dir and capture information
    for file in img_dir {
      let file = file.unwrap();
      let path = file.path();

      // skip if not a file.
      if !path.is_file() {
        continue;
      }

      let filename = file.path().as_path().file_stem().unwrap().to_str().unwrap().to_owned();
      println!("processing {}", path.as_path().display().to_string());

      // open the dataset using GDAL.
      let dataset = match Dataset::open(&path) {
        Ok(ds) => ds,
        Err(_) => continue,
      };

      let img = ImageryFile::new(&dataset, path, &filename, collection_id);

      coverage.push(img);
    }
    coverage
  }

  /// Create a new collection from a prefix in an S3 bucket.
  /// It's expected that all the collections are based on common prefixes (e.g. subfolders)
  /// in a single S3 bucket. If anybody wants to use this differently, post an issue.
  pub async fn new_from_s3_prefix(
    id: &str,
    title: &str,
    description: &str,
    s3_host: &str,
    client: &s3::Client,
    bucket: &str,
    prefix: &str
  ) -> ImageryCollection {


    // the rust AWS client expects AWS_S3_ENDPOINT to include the scheme (http/https),
    // but GDAL expects AWS_S3_ENDPOINT to only include the host/port.
    let endpoint = s3_host.parse::<http::Uri>().unwrap();
    let hostname = endpoint.authority().expect("Expected a host and port in AWS_S3_ENDPOINT").as_str();
    let _ = gdal::config::set_config_option("AWS_S3_ENDPOINT", hostname);

    let mut files = Vec::new();

    let results = client
      .list_objects()
      .bucket(bucket)
      .prefix(prefix)
      .send().await;
    
    let results = results.expect("could not get objects");

    for r in results.contents.unwrap() {
      let key = r.key.unwrap();
      let path = String::from("/vsis3/") + bucket + "/" + &key;
      let vsipath = Path::new(&path);
      let dataset = match Dataset::open(&vsipath) {
        Ok(ds) => ds,
        Err(_) => {
          println!("Failed to open {}", key);
          continue
        },
      };
      println!("processing {}", key);

      // create a link to this object on the S3 server.
      // we might need to make this more configurable (bucket.example.com vs example.com/bucket)
      let href = String::from(s3_host) + "/" + bucket + "/" + &key;
      let key_no_prefix = key.strip_prefix(&(String::from(prefix) + "/")).unwrap();

      let img = ImageryFile::new(
        &dataset,
        href.into(),
        key_no_prefix,
        prefix
      );
      files.push(img);
    }

    ImageryCollection{
      id: id.to_string(),
      title: title.to_string(),
      description: description.to_string(),
      files
    }
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
      let item = f.to_stac_feature();
      collection.links.push(item.to_stac_link(&collection_url));
    }

    collection
  }

  /// returns all the files in ImageryCollection.
  pub fn all(&self) -> &Vec<ImageryFile> {
    &self.files
  }

  /// Returns files in ImageryCollection that intersect with geom (lat/lng / EPSG:4326)
  pub fn intersects(&self, geom: &Geometry<f64>) -> Vec<ImageryFile> {
    let mut matching_files: Vec<ImageryFile> = Vec::new();
    for f in self.files.iter() {
        if f.boundary.intersects(geom) {
            matching_files.push(f.to_owned());
        }
    };
    matching_files
  }

  /// returns files in ImageryCollection whose extent contains geom (geom should use lat/lng)
  /// todo: make more generic
  pub fn contains(&self, geom: &Polygon<f64>) -> Vec<ImageryFile> {
    let mut matching_files: Vec<ImageryFile> = Vec::new();
    for f in self.files.iter() {
        if f.boundary.contains(geom) {
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
        fc.features.push(rast.to_stac_feature());
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

impl Resolution {
    // returns the simple average of the calculated x and y resolution.
    // todo: should this be diagonal resolution?
    pub fn avg(&self) -> f64 {
      (self.x + self.y) / 2.
    }
}

#[derive(Debug,Clone)]
pub struct ImageryFileProperties {
  pub path: String,
  pub filename: String,
  pub crs: String,
  pub resolution: Resolution,
  pub num_bands: u16,
  pub description: Option<String>,
  pub cloud_coverage: Option<f64>,
  pub timestamp: DateTime<Utc>,
  pub red_band: Option<u16>,
  pub ni_band: Option<u16>
}

/// metadata about images
#[derive(Debug, Clone)]
pub struct ImageryFile {
  path: PathBuf,
  filename: String,
  pub boundary: Polygon<f64>,
  pub properties: ImageryFileProperties,
  collection_id: String
}

impl ImageryFile {
    /// create a STAC ItemProperties object out of the ImageryFile's properties.
    pub fn stac_properties(&self) -> stac::ItemProperties {
      stac::ItemProperties {
        datetime: self.properties.timestamp,
        title: self.filename.to_owned(),
        description: self.properties.description.to_owned(),
        created: None, // unimplemented
        updated: None, // unimplemented
        spatial_resolution: Some(self.properties.resolution.avg())
      }
    }

    /// a GeoJSON Feature with all the fields of a STAC Item 
    pub fn to_stac_feature(&self) -> geojson::Feature {
        let geometry = geojson::Geometry::from(&self.boundary);
        let bbox_rect = self.boundary.bounding_rect().unwrap();
        let bbox: Option<Vec<f64>> = Some(vec![
          bbox_rect.min().x,
          bbox_rect.min().y,
          bbox_rect.max().x,
          bbox_rect.max().y,
        ]);
        let properties = self.stac_properties();

        let mut assets: Map<String, Value> = Map::new();

        // create the default "file" asset.
        // this points to the actual file that was catalogued.
        // in the future, it might be nice to create assets from bands.
        let file_asset: Value =  to_value(stac::ItemAsset{
            href: self.properties.path.to_owned()
        }).unwrap();
        assets.insert("file".to_string(), file_asset);

        let mut foreign_members = Map::new();
        foreign_members.insert(String::from("links"), serde_json::Value::Array(Vec::new()));
        foreign_members.insert(String::from("assets"), to_value(assets).unwrap());
        foreign_members.insert(String::from("collection"), serde_json::Value::String(self.collection_id.to_owned()));

        Feature {
            id: Some(geojson::feature::Id::String(self.filename.to_owned())),
            bbox,
            geometry: Some(geometry),
            properties: Some(properties.to_map()),
            foreign_members: Some(foreign_members)
        }
    }

    /// Creates a new ImageryFile from a GDAL Dataset
    pub fn new(dataset: &Dataset, path: PathBuf, filename: &str, collection_id: &str) -> ImageryFile {
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
      // Also need a default value for when timestamp isn't available. Right now we're using 1900/1/1.
      let timestamp: DateTime<Utc> = match dataset
          .metadata_item("PRODUCT_START_TIME", "") {
            Some(ts) => DateTime::parse_from_rfc3339(&ts).unwrap().with_timezone(&Utc),
            None => Utc.ymd(1900, 1, 1).and_hms(0, 0, 0),
          };


      // capture the IMAGEDESCRIPTION tag.
      let description: Option<String> = dataset
          .metadata_item("TIFFTAG_IMAGEDESCRIPTION", "");

      // convert extent polygon into lat/long
      let boundary: Polygon<f64> = transform::transform_polygon(&poly, &crs, "EPSG:4326");

      // add the file information to the coverage vector.
      let properties = ImageryFileProperties {
          path: path.as_path().display().to_string(),
          filename: filename.to_string(),
          crs: crs.to_owned(),
          resolution: get_resolution(&dataset),
          description,
          num_bands,
          cloud_coverage,
          timestamp,
          red_band: None, // unimplemented
          ni_band: None  // unimplemented
      };

      ImageryFile{
          path,
          filename: filename.to_string(),
          boundary,
          properties,
          collection_id: collection_id.to_owned()
      }
    }
}

/// get_resolution uses a raster image's geotransform to determine the resolution.
/// https://gdal.org/tutorials/geotransforms_tut.html
fn get_resolution(dataset: &Dataset) -> Resolution {
  let crs = dataset.projection();
  let [xmin, xsize, xskew, ymax, yskew, ysize] = dataset.geo_transform().unwrap();
  let mut xpixel = (xsize.powi(2) + xskew.powi(2)).sqrt();
  let mut ypixel = (ysize.powi(2) + yskew.powi(2)).sqrt();

  // upper left
  let p0 = point!(
    x:    xmin,
    y:    ymax 
  );
  let p1 = transform::transform_point(p0, &crs, "EPSG:4326");

  // HELP!  How do we check if our coordinates are latlng?
  // this is a terrible method and can probably give a false positive in some areas.
  println!("{:?}", crs);
  if xmin.abs() < 180. && ymax.abs() < 90. && (p1.x() - p0.x()).abs() < 0.000001 {
    // upper right
    let x1 = point!(
      x: xmin + xpixel,
      y: ymax
    );
    let x1 = transform::transform_point(x1, &crs, "EPSG:4326");

    println!("{:?}", ypixel);
    // lower left
    let y1 = point!(
      x: xmin,
      y: ymax + ypixel
    );
    let y1 = transform::transform_point(y1, &crs, "EPSG:4326");
    xpixel = p0.haversine_distance(&x1);
    ypixel = p0.haversine_distance(&y1);
  }



  Resolution{x: xpixel, y: ypixel}
}

/// get_extent calculates the extent of a given dataset and
/// returns a geo_types::Polygon representing it.
fn get_extent(dataset: &Dataset) -> Polygon<f64> {
  let [xmin, x_size, _, ymin, _, y_size] = dataset.geo_transform().unwrap();
  let (width, height) = dataset.raster_size();

  // this calculation tosses out skew, but incorporating the pixel widths from
  // get_resolution (which include skew) seems to return incorrect results.
  // TODO: get a test for both functions asap.
  let xmax = xmin + width as f64 * x_size;
  let ymax = ymin + height as f64 * y_size;
  polygon![
      (x: xmin, y: ymin),
      (x: xmax, y: ymin),
      (x: xmax, y: ymax),
      (x: xmin, y: ymax)
  ]
}

/// looks for folders within `dir` and creates collections out of them.
/// currently, this means that you should create a data directory that
/// itself contains one or more folders that represent collections.
/// e.g. the following 3 folders under the ./data dir:
///   ./data/imagery
///   ./data/dem
///   ./data/sentinel2
/// would create collections "imagery", "dem", and "sentinel2".
pub fn collections_from_subdirs(dir: &str) -> HashMap<String, ImageryCollection> {
  let mut collections: HashMap<String, ImageryCollection> = HashMap::new();
  let data_dir = fs::read_dir(dir).unwrap();

  for entry in data_dir {
    let file = entry.unwrap();
    let path = file.path();

    // skip if not a file.
    if !path.is_dir() {
      continue;
    }

    // yikes
    let dirname = file.path().as_path().file_stem().unwrap().to_str().unwrap().to_owned();

    let c = ImageryCollection::new_from_dir(
      dirname.to_owned(),
      dirname.to_owned(),
      dirname.to_owned(),
      path
    );
    collections.insert(dirname, c);
  }
  collections
}

/* S3 integration */

/// Creates collections from an S3 bucket.
/// Collections are created from object prefixes.
/// For now, objects need to have a prefix to get put into a collection, e.g.:
/// mybucket/imagery/img1.tif will put img1.tif into an `imagery` collection.
pub async fn collections_from_s3(
  s3_host: &str,
  s3_bucket: &str,
  s3_access_key: &str,
  s3_secret_key: &str
) -> HashMap<String, ImageryCollection> {
  let mut collections: HashMap<String, ImageryCollection> = HashMap::new();
  
  
  let creds = s3::Credentials::from_keys(s3_access_key, s3_secret_key, None);

  let region = s3::Region::new("us-west-1");
  let uri = s3_host.parse::<http::Uri>().unwrap();
  let s3_config = s3::Config::builder()
      .region(region)
      .endpoint_resolver(s3::Endpoint::immutable(uri))
      // .credentials_provider(s3::CredentialsProvider)
      .credentials_provider(creds)
      .build();

  let s3_client = s3::Client::from_conf(s3_config);

  println!("Scanning S3 bucket {} for collections of images", s3_bucket);

  let results = s3_client
    .list_objects()
    .bucket(s3_bucket)
    .delimiter("/")
    .send().await;

  let prefixes = results.unwrap().common_prefixes;

  for p in prefixes.unwrap() {
      let prefix_name = p.prefix.unwrap().trim_end_matches('/').to_owned();
      let c = ImageryCollection::new_from_s3_prefix(
        &prefix_name,
        &prefix_name, // in the future, a discoverable config file might be nice.
        &prefix_name,
        &s3_host,
        &s3_client,
        s3_bucket,
        &prefix_name
      ).await;
      collections.insert(prefix_name.to_string(), c);
  }


  collections
}
