use std::f64;
use std::path::PathBuf;
use std::fs;
use chrono::DateTime;
use chrono::offset::FixedOffset;
use geo::polygon;
use geo::algorithm::intersects::Intersects;
use gdal::{Dataset, Metadata};
use geojson::Feature;
use geojson::FeatureCollection;
use geojson::Geometry;
use proj::Proj;
use geo_types::Polygon;
use serde_json::{Map, Value, to_value};
use serde::{Serialize};

use crate::transform;

/// Service represents the raster service, with repositories for spectral imagery and raster data.
/// The categories and category names are subject to change.
/// For v0.0.1, the idea is that imagery of various sources can be filtered, or automatically
/// chosen based on resolution, least cloud coverage, or date ranges. Rasters represent
/// more varied thematic data like digital elevation models, or derived products (hillshade).
/// These files can be catalogued by the TIFFTAG_IMAGEDESCRIPTION GeoTIFF tag, and users can
/// make queries like "what resolution of DEM coverage is here".
#[derive(Debug)]
pub struct Service {
    pub imagery: ImageryRepository,
    pub rasters: RasterRepository
}

/// Convert a list of imagery metadata into a GeoJSON FeatureCollection
pub trait AsFeatureCollection {
  /// converts a collection of files into a GeoJSON FeatureCollection
  fn as_feature_collection(self) -> FeatureCollection;
}

/// RasterRepository stores metadata about raster files.
/// in the future, other storage (besides an in-memory vec) may
/// be supported.
#[derive(Debug)]
pub struct RasterRepository {
  files: Vec<RasterFile>
}

impl RasterRepository {
  pub fn new() -> RasterRepository {
    let files = RasterRepository::collect_files();
    RasterRepository{
      files
    }
  }

  /// Collect raster files in the /data/raster/ dir and return a vec of
  /// RasterFile metadata.
  /// This path should be a CLI argument before v0.0.1.
  pub fn collect_files() -> Vec<RasterFile> {
    let raster_dir = fs::read_dir("./data/raster/").unwrap();
    let mut coverage = Vec::new();
  
    for file in raster_dir {
      let filename = file.unwrap().path();
      println!("{}", filename.display());
  
      // Open the dataset using GDAL.
      // Panics if cannot be opened by GDAL.  TODO: fix before v0.0.1!
      let dataset = Dataset::open(&filename).unwrap();
      let poly = get_extent(&dataset);
      let crs = dataset.projection();
      let num_bands = dataset.raster_count() as u16;
      
      // Capture the IMAGEDESCRIPTION tag. We can allow users to
      // set this tag as a basic way to group images. e.g. "DEM",
      // "Stream_Burned_DEM". The tag value is up to the user.
      let description: Option<String> = dataset
          .metadata_item("TIFFTAG_IMAGEDESCRIPTION", "");
  
      // Convert extent polygon into EPGS:3857 web mercator
      // web mercator is used for convenient use with web maps (showing the extents
      // on a map), but this could change (lat/long?)
      let transform_to_3857 = Proj::new_known_crs(&crs, "EPSG:3857", None).unwrap();
      let boundary: Polygon<f64> = transform::transform_polygon(transform_to_3857, &poly);
      // Add the file information to the coverage vector.
      let properties = RasterFileProperties {
          filename: filename.as_path().display().to_string(),
          crs,
          resolution: get_resolution_from_geotransform(&dataset.geo_transform().unwrap()),
          description,
          num_bands,
      };
      let file = RasterFile{
          filename,
          boundary,
          properties
      };
      coverage.push(file);
    }
    coverage
  }

  /// Returns all the files in RasterRepository.
  pub fn all(&self) -> &Vec<RasterFile> {
    &self.files
  }

  /// Return files in RasterRepository that intersects with bounds.
  /// currently, bounds must be web mercator (EPSG:3857).
  pub fn intersects(&self, bounds: &Polygon<f64>) -> Vec<RasterFile> {
    let mut matching_files: Vec<RasterFile> = Vec::new();
    for f in self.files.iter() {
        if f.boundary.intersects(bounds) {
            matching_files.push(f.to_owned());
        }
    };
    matching_files
  }
}

/// ImageryRepository stores metadata about spectral imagery files such as
/// satellite imagery.
#[derive(Debug)]
pub struct ImageryRepository {
  files: Vec<ImageryFile>
}

impl ImageryRepository {
  /// Create a new ImageryRepository, populated with files found by
  /// collect_files.
  pub fn new() -> ImageryRepository {
    let files = ImageryRepository::collect_files();
    ImageryRepository{
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
        let filename = file.unwrap().path();
        println!("{}", filename.display());

        // open the dataset using GDAL.
        let dataset = Dataset::open(&filename).unwrap();
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

        // convert extent polygon into EPGS:3857 web mercator
        let transform_to_3857 = Proj::new_known_crs(&crs, "EPSG:3857", None).unwrap();
        let boundary: Polygon<f64> = transform::transform_polygon(transform_to_3857, &poly);

        // add the file information to the coverage vector.
        let properties = ImageryFileProperties {
            filename: filename.as_path().display().to_string(),
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
            filename,
            boundary,
            properties
        };
        coverage.push(file);
    }
    coverage
  }

  /// returns all the files in ImageryRepository.
  pub fn all(&self) -> &Vec<ImageryFile> {
    &self.files
  }

  /// Returns files in ImageryRepository that intersect with bounds (EPSG:3857)
  pub fn intersects(&self, bounds: &Polygon<f64>) -> Vec<ImageryFile> {
    let mut matching_files: Vec<ImageryFile> = Vec::new();
    for f in self.files.iter() {
        if f.boundary.intersects(bounds) {
            matching_files.push(f.to_owned());
        }
    };
    matching_files
  }
}

impl AsFeatureCollection for &Vec<RasterFile> {
  /// converts a vec of RasterFiles into a FeatureCollection
  fn as_feature_collection(self) -> FeatureCollection {
    let mut fc = FeatureCollection {
      bbox: None,
      features: vec![],
      foreign_members: None
    };
    for img in self {
        let geometry = Geometry::from(&img.boundary);

        let properties = img.properties.to_map();

        let feat = Feature {
            id: None,
            bbox: None,
            geometry: Some(geometry),
            properties: Some(properties),
            foreign_members: None
        };
        fc.features.push(feat);
    };
    fc
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

        let properties = rast.properties.to_map();

        let feat = Feature {
            id: None,
            bbox: None,
            geometry: Some(geometry),
            properties: Some(properties),
            foreign_members: None
        };
        fc.features.push(feat);
    };
    fc
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

/// metadata about spectral images such as satellite imagery
#[derive(Debug, Clone)]
pub struct ImageryFile {
  filename: PathBuf,
  pub boundary: Polygon<f64>,
  pub properties: ImageryFileProperties
}

#[derive(Debug, Clone)]
pub struct RasterFileProperties {
    pub filename: String,
    pub crs: String,
    pub resolution: Resolution,
    pub num_bands: u16,
    pub description: Option<String>
  }

impl RasterFileProperties {
    pub fn to_map(&self) -> Map<String, Value> {
        let mut properties = Map::new();
        properties.insert(String::from("filename"), to_value(&self.filename).unwrap());
        properties.insert(String::from("crs"), to_value(&self.crs).unwrap());
        properties.insert(String::from("resolution"), to_value(&self.resolution).unwrap());
        properties.insert(String::from("description"), to_value(&self.description).unwrap());
        properties.insert(String::from("num_bands"), to_value(&self.num_bands).unwrap());
        properties
    }
}

/// metadata about a raster data image.
/// this is "thematic" data such as a DEM or a DEM-derived product.
#[derive(Debug, Clone)]
pub struct RasterFile {
    pub filename: PathBuf,
    pub boundary: Polygon<f64>,
    pub properties: RasterFileProperties
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
