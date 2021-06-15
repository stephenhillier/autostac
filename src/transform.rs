use geo::algorithm::map_coords::MapCoords;

use proj::Proj;
use geo_types::Polygon;

pub fn transform_polygon(func: Proj, poly: &Polygon<f64>) -> Polygon<f64> {
  let p2 = poly
    .map_coords(|&x| func.convert(x).unwrap());
  return p2
}
