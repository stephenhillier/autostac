#[macro_use] extern crate rocket;
use std::f64;
use std::convert::TryInto;
use std::path::PathBuf;
use std::process::Command;
use std::fs;
use std::u32;
use std::u8;
use geo::polygon;
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
fn get_resolution_from_geotransform(geotransform: Vec<f64>) -> Resolution {
    let xwidth = (geotransform[1].powi(2) + geotransform[2].powi(2)).sqrt();
    let ywidth = (geotransform[5].powi(2) + geotransform[4].powi(2)).sqrt();
    Resolution{x: xwidth, y: ywidth}
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
    for path in img_dir {
        let name = path.unwrap().path();
        println!("{}", name.display());

        // run gdalinfo and capture the output into a GDALInfo instance.
        let output = Command::new("gdalinfo")
                .arg(&name)
                    .arg("-json")
                .output()
                .expect("gdalinfo failed");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let img: GDALInfo = serde_json::from_str(&stdout).unwrap();

        // turn the wgs84Extent value into a Polygon
        let poly: Polygon<f64> = img.wgs84_extent.value.try_into().unwrap();
        
        // convert extent polygon into EPGS:3857 web mercator
        let transform_4326_3857 = Proj::new_known_crs("EPSG:4326", "EPSG:3857", None).unwrap();
        let poly_mercator: Polygon<f64> = transform::transform_polygon(transform_4326_3857, &poly);

        // add the file information to the coverage vector.
        let file = ImageryFile{
            filename: name,
            kind: FileKind::Imagery,
            boundary: poly_mercator,
            resolution: get_resolution_from_geotransform(img.geo_transform),
            num_bands: 1, // unimplemented
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
