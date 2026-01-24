//! Single DEM tile representation.

use crate::{DemError, Result};
use std::path::Path;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::tags::Tag;

/// A single DEM tile loaded from a GeoTIFF file.
///
/// USGS 1/3 arc-second tiles are typically 10812 x 10812 pixels covering
/// 1 degree of latitude and longitude.
#[derive(Debug)]
pub struct DemTile {
    /// Elevation data in row-major order (north to south, west to east).
    data: Vec<f32>,
    /// Width of the tile in pixels.
    width: u32,
    /// Height of the tile in pixels.
    height: u32,
    /// Geographic bounds.
    bounds: TileBounds,
    /// No-data value (elevations equal to this should be treated as missing).
    no_data_value: Option<f32>,
}

/// Geographic bounds of a tile.
#[derive(Debug, Clone, Copy)]
pub struct TileBounds {
    /// Minimum latitude (south edge).
    pub min_lat: f64,
    /// Maximum latitude (north edge).
    pub max_lat: f64,
    /// Minimum longitude (west edge).
    pub min_lon: f64,
    /// Maximum longitude (east edge).
    pub max_lon: f64,
}

impl TileBounds {
    /// Check if a coordinate is within the bounds.
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.min_lat && lat <= self.max_lat && lon >= self.min_lon && lon <= self.max_lon
    }
}

impl DemTile {
    /// Load a DEM tile from a GeoTIFF file.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)?;
        let mut decoder = Decoder::new(file)?;

        // Set limits to allow large DEM files
        // 1/3 arc-second tiles are 10812 x 10812 pixels = ~116 million pixels
        // Each pixel is f32 (4 bytes) = ~466 MB
        let mut limits = Limits::default();
        limits.decoding_buffer_size = 1024 * 1024 * 1024; // 1 GB
        limits.intermediate_buffer_size = 1024 * 1024 * 1024; // 1 GB
        limits.ifd_value_size = 1024 * 1024 * 1024;
        decoder = decoder.with_limits(limits);

        let (width, height) = decoder.dimensions()?;

        // Try to read geotransform from GeoTIFF tags
        let bounds = Self::read_geotransform(&mut decoder, path)?;

        // Read the elevation data
        let data = Self::decode_elevation_data(&mut decoder)?;

        // Try to read no-data value (GDAL_NODATA tag = 42113)
        let no_data_value = Self::read_nodata_value(&mut decoder);

        Ok(Self {
            data,
            width,
            height,
            bounds,
            no_data_value,
        })
    }

    /// Load a DEM tile from a GeoTIFF file with explicit bounds.
    ///
    /// Use this when the tile doesn't have GeoTIFF tags or the filename
    /// doesn't follow the USGS naming convention (e.g., AWS terrain tiles).
    pub fn from_file_with_bounds<P: AsRef<Path>>(path: P, bounds: TileBounds) -> Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)?;
        let mut decoder = Decoder::new(file)?;

        // Set limits to allow large DEM files
        let mut limits = Limits::default();
        limits.decoding_buffer_size = 1024 * 1024 * 1024; // 1 GB
        limits.intermediate_buffer_size = 1024 * 1024 * 1024; // 1 GB
        limits.ifd_value_size = 1024 * 1024 * 1024;
        decoder = decoder.with_limits(limits);

        let (width, height) = decoder.dimensions()?;

        // Read the elevation data
        let data = Self::decode_elevation_data(&mut decoder)?;

        // Try to read no-data value (GDAL_NODATA tag = 42113)
        let no_data_value = Self::read_nodata_value(&mut decoder);

        Ok(Self {
            data,
            width,
            height,
            bounds,
            no_data_value,
        })
    }

    /// Read the geotransform (geographic bounds) from GeoTIFF tags.
    fn read_geotransform<R: std::io::Read + std::io::Seek>(
        decoder: &mut Decoder<R>,
        path: &Path,
    ) -> Result<TileBounds> {
        // First try to read ModelTiepoint (tag 33922) and ModelPixelScale (tag 33550)
        let tiepoint = decoder.get_tag_f64_vec(Tag::Unknown(33922));
        let pixel_scale = decoder.get_tag_f64_vec(Tag::Unknown(33550));

        if let (Ok(tiepoint), Ok(scale)) = (tiepoint, pixel_scale) {
            if tiepoint.len() >= 6 && scale.len() >= 2 {
                // Tiepoint format: [i, j, k, x, y, z] where (i,j) is pixel coords and (x,y) is geo coords
                let tie_x = tiepoint[3]; // Longitude of tie point
                let tie_y = tiepoint[4]; // Latitude of tie point
                let scale_x = scale[0]; // Degrees per pixel (longitude)
                let scale_y = scale[1]; // Degrees per pixel (latitude)

                let (width, height) = decoder.dimensions()?;

                // Top-left corner is the tiepoint, data goes south and east
                let max_lat = tie_y;
                let min_lat = tie_y - (height as f64 * scale_y);
                let min_lon = tie_x;
                let max_lon = tie_x + (width as f64 * scale_x);

                return Ok(TileBounds {
                    min_lat,
                    max_lat,
                    min_lon,
                    max_lon,
                });
            }
        }

        // Fallback: try to parse from filename
        Self::bounds_from_filename(path)
    }

    /// Parse tile bounds from a USGS filename like "USGS_13_n48w123_*.tif".
    fn bounds_from_filename(path: &Path) -> Result<TileBounds> {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| DemError::InvalidFilename(path.display().to_string()))?;

        // Pattern: USGS_13_n##w###_*.tif or n##w###.tif
        // Find the n##w### pattern
        let mut chars = filename.chars().peekable();
        let mut found_n = false;
        let mut lat_str = String::new();
        let mut lon_str = String::new();
        let mut is_north = true;
        let mut is_west = true;

        while let Some(c) = chars.next() {
            if c == 'n' || c == 's' {
                is_north = c == 'n';
                // Read latitude digits
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        lat_str.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                // Expect 'w' or 'e'
                if let Some(dir) = chars.next() {
                    is_west = dir == 'w';
                    // Read longitude digits
                    while let Some(&d) = chars.peek() {
                        if d.is_ascii_digit() {
                            lon_str.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                    found_n = true;
                    break;
                }
            }
        }

        if !found_n || lat_str.is_empty() || lon_str.is_empty() {
            return Err(DemError::InvalidFilename(filename.to_string()));
        }

        let lat: f64 = lat_str
            .parse()
            .map_err(|_| DemError::InvalidFilename(filename.to_string()))?;
        let lon: f64 = lon_str
            .parse()
            .map_err(|_| DemError::InvalidFilename(filename.to_string()))?;

        // The filename indicates the northwest corner of the tile
        // Tile covers 1 degree in each direction
        let min_lat = if is_north { lat - 1.0 } else { -(lat + 1.0) };
        let max_lat = if is_north { lat } else { -lat };
        let min_lon = if is_west { -lon } else { lon - 1.0 };
        let max_lon = if is_west { -(lon - 1.0) } else { lon };

        Ok(TileBounds {
            min_lat,
            max_lat,
            min_lon,
            max_lon,
        })
    }

    /// Decode elevation data from the TIFF decoder.
    fn decode_elevation_data<R: std::io::Read + std::io::Seek>(
        decoder: &mut Decoder<R>,
    ) -> Result<Vec<f32>> {
        let result = decoder.read_image()?;

        match result {
            DecodingResult::F32(data) => Ok(data),
            DecodingResult::F64(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::I16(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::I32(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::U16(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::U32(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::U8(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::I8(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::U64(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
            DecodingResult::I64(data) => Ok(data.into_iter().map(|v| v as f32).collect()),
        }
    }

    /// Try to read the no-data value from GDAL_NODATA tag.
    fn read_nodata_value<R: std::io::Read + std::io::Seek>(decoder: &mut Decoder<R>) -> Option<f32> {
        // GDAL_NODATA tag is 42113, stored as ASCII string
        if let Ok(nodata_str) = decoder.get_tag_ascii_string(Tag::Unknown(42113)) {
            nodata_str.parse().ok()
        } else {
            // Common default for USGS DEMs
            Some(-999999.0)
        }
    }

    /// Get the elevation at a geographic coordinate.
    ///
    /// Uses bilinear interpolation between the four nearest pixels.
    ///
    /// # Arguments
    /// * `lat` - Latitude in decimal degrees (positive = north)
    /// * `lon` - Longitude in decimal degrees (negative = west)
    ///
    /// # Returns
    /// Elevation in meters, or an error if the coordinate is out of bounds.
    pub fn get_elevation(&self, lat: f64, lon: f64) -> Result<f32> {
        if !self.bounds.contains(lat, lon) {
            return Err(DemError::OutOfBounds {
                lat,
                lon,
                min_lat: self.bounds.min_lat,
                max_lat: self.bounds.max_lat,
                min_lon: self.bounds.min_lon,
                max_lon: self.bounds.max_lon,
            });
        }

        // Convert geographic coordinates to pixel coordinates
        // Note: row 0 is at max_lat (north), increases southward
        let lat_range = self.bounds.max_lat - self.bounds.min_lat;
        let lon_range = self.bounds.max_lon - self.bounds.min_lon;

        let x = ((lon - self.bounds.min_lon) / lon_range) * (self.width - 1) as f64;
        let y = ((self.bounds.max_lat - lat) / lat_range) * (self.height - 1) as f64;

        // Bilinear interpolation
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);

        let fx = x - x0 as f64;
        let fy = y - y0 as f64;

        let v00 = self.get_pixel(x0, y0)?;
        let v10 = self.get_pixel(x1, y0)?;
        let v01 = self.get_pixel(x0, y1)?;
        let v11 = self.get_pixel(x1, y1)?;

        // Bilinear interpolation formula
        let elevation = v00 as f64 * (1.0 - fx) * (1.0 - fy)
            + v10 as f64 * fx * (1.0 - fy)
            + v01 as f64 * (1.0 - fx) * fy
            + v11 as f64 * fx * fy;

        Ok(elevation as f32)
    }

    /// Get the raw elevation value at a pixel coordinate (no interpolation).
    ///
    /// # Arguments
    /// * `lat` - Latitude in decimal degrees
    /// * `lon` - Longitude in decimal degrees
    pub fn get_elevation_nearest(&self, lat: f64, lon: f64) -> Result<f32> {
        if !self.bounds.contains(lat, lon) {
            return Err(DemError::OutOfBounds {
                lat,
                lon,
                min_lat: self.bounds.min_lat,
                max_lat: self.bounds.max_lat,
                min_lon: self.bounds.min_lon,
                max_lon: self.bounds.max_lon,
            });
        }

        let lat_range = self.bounds.max_lat - self.bounds.min_lat;
        let lon_range = self.bounds.max_lon - self.bounds.min_lon;

        let x = (((lon - self.bounds.min_lon) / lon_range) * (self.width - 1) as f64).round() as u32;
        let y =
            (((self.bounds.max_lat - lat) / lat_range) * (self.height - 1) as f64).round() as u32;

        self.get_pixel(x, y)
    }

    /// Get the elevation at a pixel coordinate.
    fn get_pixel(&self, x: u32, y: u32) -> Result<f32> {
        let idx = (y * self.width + x) as usize;
        let value = self.data[idx];

        // Check for no-data value
        if let Some(nodata) = self.no_data_value {
            if (value - nodata).abs() < 0.001 {
                return Err(DemError::NoData {
                    lat: 0.0,
                    lon: 0.0,
                });
            }
        }

        Ok(value)
    }

    /// Get the geographic bounds of this tile.
    pub fn bounds(&self) -> TileBounds {
        self.bounds
    }

    /// Get the dimensions of this tile in pixels.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Get the resolution in degrees per pixel.
    pub fn resolution(&self) -> (f64, f64) {
        let lat_range = self.bounds.max_lat - self.bounds.min_lat;
        let lon_range = self.bounds.max_lon - self.bounds.min_lon;
        (lon_range / self.width as f64, lat_range / self.height as f64)
    }

    /// Get the approximate resolution in meters at the center of the tile.
    pub fn resolution_meters(&self) -> (f64, f64) {
        let (lon_deg, lat_deg) = self.resolution();
        let center_lat = (self.bounds.min_lat + self.bounds.max_lat) / 2.0;

        // At the equator, 1 degree ≈ 111,320 meters
        // Longitude shrinks by cos(latitude)
        let meters_per_deg_lat = 111_320.0;
        let meters_per_deg_lon = 111_320.0 * center_lat.to_radians().cos();

        (lon_deg * meters_per_deg_lon, lat_deg * meters_per_deg_lat)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bounds_from_filename() {
        // Test n48w123 - should cover lat 47-48, lon -123 to -122
        let bounds = DemTile::bounds_from_filename(Path::new("USGS_13_n48w123_20240327.tif"))
            .expect("Should parse filename");

        assert_eq!(bounds.min_lat, 47.0);
        assert_eq!(bounds.max_lat, 48.0);
        assert_eq!(bounds.min_lon, -123.0);
        assert_eq!(bounds.max_lon, -122.0);
    }

    #[test]
    fn test_bounds_contains() {
        let bounds = TileBounds {
            min_lat: 47.0,
            max_lat: 48.0,
            min_lon: -123.0,
            max_lon: -122.0,
        };

        assert!(bounds.contains(47.5, -122.5));
        assert!(bounds.contains(47.0, -123.0)); // Corner
        assert!(bounds.contains(48.0, -122.0)); // Corner
        assert!(!bounds.contains(46.5, -122.5)); // Too far south
        assert!(!bounds.contains(48.5, -122.5)); // Too far north
        assert!(!bounds.contains(47.5, -121.5)); // Too far east
        assert!(!bounds.contains(47.5, -123.5)); // Too far west
    }
}
