#[macro_use] extern crate rocket;
use std::f64;
use std::path::PathBuf;
use std::fs;
use std::u32;
use std::u8;
use chrono::DateTime;
use chrono::offset::FixedOffset;
use geo::polygon;
use geojson::{Geometry, Feature, FeatureCollection};
use gdal::{Dataset, Metadata};
use proj::Proj;
use geo::algorithm::intersects::Intersects;
use geo_types::Polygon;
use serde_json::{Map, Value, to_value, to_string};
use serde::{Serialize};
use rocket::State;
use tile_grid::Grid;

mod transform;

// types of files we may encounter
#[derive(Debug, Clone)]
enum FileKind {
    Imagery
}

// Resolution represents the horizontal (x) and vertical (y)
// length of a single pixel, in the map units.
// TODO:  this needs to be converted to m, not use the base map units.
#[derive(Debug, Clone, Copy, Serialize)]
struct Resolution {
    y: f64,
    x: f64
}

// DataFile represents a file that has been checked and catalogued
// and is available to use for tile requests.
#[derive(Debug)]
struct DataFiles {
    imagery_files: Vec<ImageryFile>
}

#[derive(Debug, Clone, Serialize)]
struct ImageryFileProperties {
    filename: String,
    resolution: Resolution,
    num_bands: u16,
    cloud_coverage: Option<f64>,
    timestamp: Option<DateTime<FixedOffset>>,
    red_band: Option<u16>,
    ni_band: Option<u16>
}

#[derive(Debug, Clone)]
struct ImageryFile {
    filename: PathBuf,
    kind: FileKind,
    boundary: Polygon<f64>,
    properties: ImageryFileProperties
}

// get_tile_bounds returns the tile boundaries (web mercator EPSG:3857)
// for a tile from a URL with /z/x/y format.
fn get_tile_bounds(z: u8, x:u32, y:u32) -> Polygon<f64> {
    let grid = Grid::web_mercator();
    let extent = grid.tile_extent_xyz(x, y, z);
    println!("z:{}, x:{}, y:{}, extent: {:?}", z, x, y, extent);
    polygon!(
        (x: extent.minx, y: extent.miny),
        (x: extent.maxx, y: extent.miny),
        (x: extent.maxx, y: extent.maxy),
        (x: extent.minx, y: extent.maxy),
    )
}

// get_resolution_from_geotransform uses a raster image's geotransform
// to determine the resolution.
// https://gdal.org/tutorials/geotransforms_tut.html
fn get_resolution_from_geotransform(geotransform: &[f64]) -> Resolution {
    let xwidth = (geotransform[1].powi(2) + geotransform[2].powi(2)).sqrt();
    let ywidth = (geotransform[5].powi(2) + geotransform[4].powi(2)).sqrt();
    Resolution{x: xwidth, y: ywidth}
}

// get_extent calculates the extent of a given dataset and
// returns a geo_types::Polygon representing it.
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

// register_images searches the imagery directory and collects
// metadata about valid images.  Images are valid if they can be
// opened by GDAL.
fn register_images() -> Vec<ImageryFile> {
    // just a hardcoded dir path for now.  Put satellite imagery (e.g. Sentinel-2)
    // into this folder.
    let img_dir = fs::read_dir("./data/imagery/").unwrap();

    let mut coverage: Vec<ImageryFile> = Vec::new();

    // iterate through the files in img_dir and capture information
    // using gdalinfo.  We need to get the extent and the geotransform
    // (to calculate the image resolution).
    for file in img_dir {
        let filename = file.unwrap().path();
        println!("{}", filename.display());

        // open the dataset using GDAL.
        // panics if cannot be opened by GDAL.  TODO: fix before v0.0.1!
        let dataset = Dataset::open(&filename).unwrap();
        let poly = get_extent(&dataset);
        let projection = dataset.projection();
        let num_bands = dataset.raster_count() as u16;
        
        //Check metadata for cloud coverage
        let cloud_coverage: Option<f64> = match dataset.metadata_item("CLOUD_COVERAGE_ASSESSMENT", "") {
            Some(s) => Some(s.parse::<f64>().unwrap()),
            None => None,
        };

        let timestamp: Option<DateTime<FixedOffset>> = match dataset.metadata_item("PRODUCT_START_TIME", "") {
            Some(s) => Some(DateTime::parse_from_rfc3339(&s).unwrap()),
            None => None,
        };



        // convert extent polygon into EPGS:3857 web mercator
        let transform_4326_3857 = Proj::new_known_crs(&projection, "EPSG:3857", None).unwrap();
        let boundary: Polygon<f64> = transform::transform_polygon(transform_4326_3857, &poly);
        println!("{:?}", boundary);
        // add the file information to the coverage vector.
        let properties = ImageryFileProperties {
            filename: filename.as_path().display().to_string(),
            resolution: get_resolution_from_geotransform(&dataset.geo_transform().unwrap()),
            num_bands,
            cloud_coverage,
            timestamp,
            red_band: None, // unimplemented
            ni_band: None  // unimplemented
        };

        let file = ImageryFile{
            filename,
            kind: FileKind::Imagery,
            boundary,
            properties
        };
        coverage.push(file);
    }
    coverage
}

// files_intersecting_bounds returns the subset of files from `file_list` that intersect
// with `bounds`.
fn files_intersecting_bounds(file_list: &[ImageryFile], bounds: &Polygon<f64>) -> Vec<ImageryFile> {
    let mut matching_files: Vec<ImageryFile> = Vec::new();
    for f in file_list.iter() {
        if f.boundary.intersects(bounds) {
            matching_files.push(f.to_owned());
        }
    };
    matching_files
}

fn imagery_properties_to_map(props: &ImageryFileProperties) -> Map<String,Value> {
    let mut properties = Map::new();

    // silly way to create properties map...
    // need to fix (find a better way to convert to a format that fits in Feature.properties)
    properties.insert(String::from("filename"), to_value(&props.filename).unwrap());
    properties.insert(String::from("resolution"), to_value(&props.resolution).unwrap());
    properties.insert(String::from("num_bands"), to_value(&props.num_bands).unwrap());
    properties.insert(String::from("cloud_coverage"), to_value(&props.cloud_coverage).unwrap());
    properties.insert(String::from("timestamp"), to_value(&props.timestamp).unwrap());
    properties.insert(String::from("red_band"), to_value(&props.red_band).unwrap());
    properties.insert(String::from("ni_band"), to_value(&props.ni_band).unwrap());
    properties
}

#[get("/tile/<z>/<x>/<y>")]
fn tile(z: u8, x:u32, y:u32, coverage: &State<DataFiles>) -> String {
    let bounds = get_tile_bounds(z, x, y);
    let files_for_tile = files_intersecting_bounds(&coverage.inner().imagery_files, &bounds);

    // stand-in for an actual tile
    format!("{} {} {} :\n {:?} :\n {:?}", z, x, y, bounds, files_for_tile)
}

// returns a set of GeoJSON FeatureCollections representing
// data collections grouped into categories.
// So far only imagery (satellite imagery etc) is supported.
#[get("/collections")]
fn collections(coverage: &State<DataFiles>) -> String {
    let mut fc = FeatureCollection {
        bbox: None,
        features: vec![],
        foreign_members: None
    };
    for img in &coverage.inner().imagery_files {
        let geometry = Geometry::from(&img.boundary);
        

        let properties = imagery_properties_to_map(&img.properties);

        let feat = Feature {
            id: None,
            bbox: None,
            geometry: Some(geometry),
            properties: Some(properties),
            foreign_members: None
        };
        fc.features.push(feat);
    }
    to_string(&fc).unwrap()
}

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[launch]
fn rocket() -> _ {
    // initialize raster coverage
    let coverage = DataFiles {
        imagery_files: register_images()
    };

    // start application
    rocket::build()
        .manage(coverage)
        .mount("/", routes![tile, collections, index])
}
