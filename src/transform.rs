use std::f64::consts::PI;
use geo::algorithm::map_coords::MapCoords;
use geo::polygon;
use proj::Proj;
use geo_types::{Polygon, Point, Coordinate};

pub fn transform_polygon(poly: &Polygon<f64>, from_crs: &str, to_crs: &str) -> Polygon<f64> {
  let func = Proj::new_known_crs(from_crs, to_crs, None).unwrap();
  poly.map_coords(|&x| func.convert(x).unwrap())
}

pub fn transform_point(p: Point<f64>, from_crs: &str, to_crs: &str) -> Point<f64> {
  let func = Proj::new_known_crs(from_crs, to_crs, None).unwrap();
  func.convert(p).unwrap()
}

/// convert XYZ tiles into lat/long.
/// this returns the NW / top left corner. Use x+1 and y+1 to get other corners.
/// this math was adapted from https://wiki.openstreetmap.org/wiki/Slippy_map_tilenames#Python
fn to_lng_lat(x:u32, y:u32, z: u8) -> Coordinate<f64> {
    let n = 2_f64.powi(z as i32);
    let lon = x as f64 / n * 360.0 - 180.0;
    let lat_rad = (PI * (1_f64 - 2_f64 * y as f64 / n)).sinh().atan();
    let lat = lat_rad * 180_f64 / PI;
    Coordinate{x: lon, y: lat}
}

/// to_bounds returns the lat/lng tile boundaries as a geo_types::Polygon<f64>
/// for a tile from a URL with z/x/y format.
pub fn to_bounds(x:u32, y:u32, z: u8) -> Polygon<f64> {
    polygon!(
        to_lng_lat(x, y+1, z),
        to_lng_lat(x+1, y+1, z),
        to_lng_lat(x+1, y, z),
        to_lng_lat(x, y, z)
    )
}


mod tests {
  use crate::transform::{to_lng_lat, Coordinate};
  #[test]
  fn test_to_lng_lat() {
      // test case borrowed from mercantile's first example
      // https://github.com/mapbox/mercantile
      let ul = to_lng_lat(486, 332, 10);
      let expected =  Coordinate{x: -9.140625, y: 53.33087298301705};
      assert_eq!(true, (ul.x - expected.x).abs() < 0.0000001);
      assert_eq!(true, (ul.y - expected.y).abs() < 0.0000001);
  }
}
