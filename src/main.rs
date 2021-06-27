#[macro_use] extern crate rocket;
use std::f64;
use std::path::PathBuf;
use std::fs;
use std::u32;
use std::u8;
use chrono::DateTime;
use chrono::offset::FixedOffset;
use geo::polygon;
use gdal::{Dataset, Metadata};
use proj::Proj;
use geo::algorithm::intersects::Intersects;
use geo_types::Polygon;
use rocket::State;
use tile_grid::Grid;

mod transform;

// types of files we may encounter
#[derive(Debug)]
enum FileKind {
    Imagery
}

// GDALInfo contains information about a raster file
// that has been catalogued.
#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GDALInfo {
    wgs84_extent: geojson::Geometry,
    geo_transform: Vec<f64>
}

// Resolution represents the horizontal (x) and vertical (y)
// length of a single pixel, in the map units.
// TODO:  this needs to be converted to m, not use the base map units.
#[derive(Debug, Clone, Copy)]
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

#[derive(Debug)]
struct ImageryFile {
    filename: PathBuf,
    kind: FileKind,
    boundary: Polygon<f64>,
    resolution: Resolution,
    num_bands: u16,
    cloud_coverage: Option<f64>,
    timestamp: Option<DateTime<FixedOffset>>,
    red_band: Option<u16>,
    ni_band: Option<u16>
}

// RasterAOI represents a raster image with a given area of interest.
#[derive(Debug)]
struct RasterAOI {
    filename: PathBuf, 
    boundary: Polygon<f64>,
    resolution: Resolution
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
    let geotransform = dataset.geo_transform().unwrap();
    let (width, height) = dataset.raster_size();
    let res = get_resolution_from_geotransform(&geotransform);

    let xmin = geotransform[0];
    let ymin = geotransform[3];
    let xmax = xmin + width as f64 * res.x;
    let ymax = ymin + height as f64 * res.y;
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
        // panics if cannot be opened by GDAL.  fix before v0.0.1!
        let dataset = Dataset::open(&filename).unwrap();
        let poly = get_extent(&dataset);
        let projection = dataset.projection();

        let num_bands = dataset.raster_count() as u16;
        
        //Check metadata for cloud coverage
        let cloud_coverage: Option<f64> = match dataset.metadata_item("CLOUD_COVERAGE_ASSESSMENT", "METADATA") {
            Some(s) => Some(s.parse::<f64>().unwrap()),
            None => None,
        };

        let timestamp: Option<DateTime<FixedOffset>> = match dataset.metadata_item("PRODUCT_START_TIME", "METADATA") {
            Some(s) => Some(DateTime::parse_from_rfc3339(&s).unwrap()),
            None => None,
        };



        // convert extent polygon into EPGS:3857 web mercator
        let transform_4326_3857 = Proj::new_known_crs(&projection, "EPSG:3857", None).unwrap();
        let boundary: Polygon<f64> = transform::transform_polygon(transform_4326_3857, &poly);

        // add the file information to the coverage vector.
        let file = ImageryFile{
            filename,
            kind: FileKind::Imagery,
            boundary,
            resolution: get_resolution_from_geotransform(&dataset.geo_transform().unwrap()),
            num_bands, // unimplemented
            cloud_coverage,
            timestamp,
            red_band: None, // unimplemented
            ni_band: None  // unimplemented
        };
        coverage.push(file);
    }
    coverage
}

// files_intersecting_bounds returns the subset of files from `file_list` that intersect
// with `bounds`.
fn files_intersecting_bounds(file_list: &[ImageryFile], bounds: &Polygon<f64>) -> Vec<RasterAOI> {
    let mut matching_files: Vec<RasterAOI> = Vec::new();
    for f in file_list.iter() {
        if f.boundary.intersects(bounds) {
            let aoi = RasterAOI {
                filename: f.filename.to_owned(),
                boundary: f.boundary.clone(),
                resolution: f.resolution
            };
            matching_files.push(aoi);
        }
    };
    matching_files
}

#[get("/tile/<z>/<x>/<y>")]
fn tile(z: u8, x:u32, y:u32, coverage: &State<DataFiles>) -> String {
    let bounds = get_tile_bounds(z, x, y);
    let files_for_tile = files_intersecting_bounds(&coverage.inner().imagery_files, &bounds);

    format!("{} {} {} :\n {:?} :\n {:?}", z, x, y, bounds, files_for_tile)
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
        .mount("/", routes![tile, index])
}
