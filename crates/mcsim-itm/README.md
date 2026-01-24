# mcsim-itm

Rust wrapper for the [NTIA Irregular Terrain Model (ITM)](https://github.com/NTIA/itm).

ITM predicts terrestrial radiowave propagation for frequencies between 20 MHz and 20 GHz
based on electromagnetic theory and empirical models developed by Anita Longley and Phil Rice.

## Features

- Safe Rust bindings to the ITM DLL
- Point-to-Point prediction mode with terrain profiles
- Area prediction mode for general coverage analysis
- Support for both Time/Location/Situation and Confidence/Reliability variability modes
- Extended output with intermediate calculation values
- Helper functions for terrain profile construction

## Requirements

The ITM library (`itm.dll` on Windows) must be available at runtime. Place it in one of the following locations:

- `itm/itm.dll` relative to the executable
- Current working directory
- Or specify a custom path using `Itm::from_path()`

The prebuilt DLL can be obtained from the [ITM releases page](https://github.com/NTIA/itm/releases).

## Usage

### Area Prediction Mode

```rust
use mcsim_itm::{Itm, Climate, Polarization, SitingCriteria};

let itm = Itm::new()?;

let result = itm.area_tls(
    10.0,  // TX height in meters
    2.0,   // RX height in meters
    SitingCriteria::Random,
    SitingCriteria::Random,
    10.0,  // distance in km
    90.0,  // terrain irregularity (delta_h)
    Climate::ContinentalTemperate,
    301.0, // surface refractivity N_0
    915.0, // frequency in MHz
    Polarization::Vertical,
    15.0,  // relative permittivity (epsilon)
    0.005, // conductivity (sigma) in S/m
    0,     // mode of variability
    50.0,  // time variability %
    50.0,  // location variability %
    50.0,  // situation variability %
)?;

println!("Path loss: {} dB", result.loss_db);
```

### Point-to-Point Prediction Mode

```rust
use mcsim_itm::{Itm, Climate, Polarization, TerrainProfile};

let itm = Itm::new()?;

// Build terrain profile (elevation points from TX to RX)
let mut profile = TerrainProfile::new(100.0); // 100m resolution
profile.add_elevation(50.0);  // TX location elevation
profile.add_elevation(55.0);
profile.add_elevation(60.0);
profile.add_elevation(58.0);
profile.add_elevation(52.0);
profile.add_elevation(48.0);
profile.add_elevation(45.0);
profile.add_elevation(42.0);
profile.add_elevation(40.0);
profile.add_elevation(38.0);  // RX location elevation

let pfl = profile.to_pfl();

let result = itm.p2p_tls(
    10.0,  // TX height
    2.0,   // RX height
    &pfl,
    Climate::ContinentalTemperate,
    301.0, // N_0
    915.0, // frequency MHz
    Polarization::Vertical,
    15.0,  // epsilon
    0.005, // sigma
    0,     // mdvar
    50.0,  // time %
    50.0,  // location %
    50.0,  // situation %
)?;

println!("Point-to-point loss: {} dB", result.loss_db);
```

### Ground Constants

The crate provides predefined ground constants for common terrain types:

```rust
use mcsim_itm::ground_constants;

let (epsilon, sigma) = ground_constants::AVERAGE_GROUND;
let (epsilon, sigma) = ground_constants::SEA_WATER;
let (epsilon, sigma) = ground_constants::FRESH_WATER;
```

## API Reference

### Main Functions

- `p2p_tls()` - Point-to-point with Time/Location/Situation variability
- `p2p_cr()` - Point-to-point with Confidence/Reliability variability
- `area_tls()` - Area mode with Time/Location/Situation variability
- `area_cr()` - Area mode with Confidence/Reliability variability

Extended versions (`_ex` suffix) return intermediate calculation values.

### Helper Functions

- `compute_delta_h()` - Calculate terrain irregularity from profile
- `free_space_loss()` - Calculate free space path loss

## Enums

- `Climate` - Radio climate classification (Equatorial, Continental Temperate, etc.)
- `Polarization` - Horizontal or Vertical
- `SitingCriteria` - Random, Careful, or VeryCareful
- `PropagationMode` - LineOfSight, Diffraction, or Troposcatter

## License

This crate is licensed under the MIT license. The ITM library itself is in the public domain.

## References

- [ITM GitHub Repository](https://github.com/NTIA/itm)
- [A Guide to the Use of the ITS Irregular Terrain Model](https://www.its.bldrdoc.gov/publications/details.aspx?pub=2091)
