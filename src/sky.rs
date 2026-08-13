//! A daylight sky, generated rather than loaded.
//!
//! Builds the cubemap the scene uses as its [`bevy::light::Skybox`], and the three colours that
//! stand in for it as an environment map. Nothing is downloaded, nothing is committed: an analytic
//! sky has no source imagery, so the six faces cost a few hundred thousand evaluations at startup
//! and no asset at all. Contrast [`crate`]'s starfield, whose generator exists precisely because
//! that sky *did* have source imagery to fetch and reproject — see `tools/make_skybox.py`.
//!
//! Above the horizon the model is **Preetham et al.'s analytic daylight model** (1999): a Perez
//! distribution over the sky's zenith angle and its angle to the sun, fitted so that horizon
//! brightening and the forward-scatter halo around the sun fall out of the model instead of being
//! painted on. That matters here beyond looking right, because the horizon band is exactly the part
//! of the sky that water reflects at grazing angles.
//!
//! Below the horizon is ground, and the two do not meet at a line. Ground fades into the sky's own
//! horizon colour over [`Sky::haze`], which is what the horizon looks like from altitude: distant
//! terrain loses contrast and washes out rather than ending at an edge. Blending towards the sky's
//! colour at the same azimuth — rather than towards a separately chosen fog colour — is what keeps
//! the transition seamless at any sun position, since the two sides meet at the same value by
//! construction.

use bevy::{
    asset::RenderAssetUsages,
    image::Image,
    math::Vec3,
    prelude::*,
    render::render_resource::{
        Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
    },
};

/// Edge of one cube face, in texels.
///
/// A cloudless sky is smooth everywhere except across [`Sky::haze`], which at this size still spans
/// a dozen texels. Doubling it quadruples a startup cost paid on the CPU for no visible gain.
// ponytail: fixed size. Make it a field if a sharper feature than the haze band ever goes in the
// sky — a solar disc, or clouds.
const FACE: u32 = 256;

/// Maps face-local `(u, v)` in `[-1, 1]`, `v` pointing **down**, to a direction, in wgpu's cubemap
/// layer order: +X −X +Y −Y +Z −Z.
///
/// The same convention `tools/make_skybox.py` uses, deliberately, so the two skies are
/// interchangeable and a reader only has to learn it once. It is also the only thing here that can
/// be silently wrong, which is why the tests sample the poles.
const FACES: [fn(f32, f32) -> Vec3; 6] = [
    |u, v| Vec3::new(1.0, -v, -u),
    |u, v| Vec3::new(-1.0, -v, u),
    |u, v| Vec3::new(u, 1.0, v),
    |u, v| Vec3::new(u, -1.0, -v),
    |u, v| Vec3::new(u, -v, 1.0),
    |u, v| Vec3::new(-u, -v, -1.0),
];

/// A daylight sky over flat ground.
///
/// Values come out in **cd/m², the units Bevy's photometric pipeline already works in**, so a
/// `Skybox` takes them at `brightness: 1.0` and an `EnvironmentMapLight` at `intensity: 1.0`. There
/// is deliberately no exposure factor here: the camera's `Exposure` is where a daylight scene is
/// exposed, and a second scale in front of it would only be a knob that has to be kept in step with
/// the first.
#[derive(Clone, Copy, Debug)]
pub struct Sky {
    /// Direction **towards** the sun. Shared with the scene's `DirectionalLight` so the sky's sun
    /// and the highlight the water throws cannot drift apart.
    pub sun: Vec3,
    /// Atmospheric turbidity: the ratio of total to purely molecular optical thickness. 2 is an
    /// exceptionally clear day, 3 a clear one, 6 hazy. Preetham's fit is only trustworthy up to
    /// about 10.
    pub turbidity: f32,
    /// How bright the ground is as a **fraction of the sky at the horizon**, which is what makes it
    /// a grey rather than a number in someone's chosen units: it tracks the sky automatically, and
    /// no unit slip can quietly sink it to black.
    pub ground: f32,
    /// Angular width of the band over which ground hazes into sky, in radians. **The knob that
    /// decides whether the scene reads as ground level or as altitude**, so it is the first thing
    /// to tune against a screenshot: a narrow band is a hard edge seen from a hilltop, a wide one
    /// is the washed-out horizon seen from an aircraft.
    pub haze: f32,
}

impl Default for Sky {
    fn default() -> Self {
        Self {
            sun: Vec3::new(4.0, 8.0, 4.0).normalize(),
            turbidity: 2.0,
            ground: 0.12,
            haze: 7.0_f32.to_radians(),
        }
    }
}

impl Sky {
    /// Linear RGB looking along `dir`, which need not be normalized.
    pub fn radiance(&self, dir: Vec3) -> Vec3 {
        let dir = dir.normalize_or_zero();
        let model = Preetham::new(self.sun, self.turbidity);
        self.shade(&model, dir)
    }

    /// The cubemap for a [`bevy::light::Skybox`].
    ///
    /// Built with its cube view descriptor already set, so unlike a loaded PNG — which carries no
    /// cubemap metadata and has to be reinterpreted once it lands — there is nothing to patch up
    /// afterwards and no load state to wait on.
    pub fn cubemap(&self) -> Image {
        // Hoisted out of the loop: the zenith values and normalisation are per-sky, not per-texel,
        // and rebuilding them a million and a half times over is the difference between a
        // perceptible startup hitch and none.
        let model = Preetham::new(self.sun, self.turbidity);

        let mut data = Vec::with_capacity((FACE * FACE * 6 * 8) as usize);
        for face in FACES {
            for y in 0..FACE {
                for x in 0..FACE {
                    // Pixel centres, so no texel straddles a face edge.
                    let at = |i: u32| 2.0 * (i as f32 + 0.5) / FACE as f32 - 1.0;
                    let rgb = self.shade(&model, face(at(x), at(y)).normalize());
                    for channel in [rgb.x, rgb.y, rgb.z, 1.0] {
                        data.extend_from_slice(&f16_bits(channel).to_le_bytes());
                    }
                }
            }
        }

        Image {
            texture_view_descriptor: Some(TextureViewDescriptor {
                dimension: Some(TextureViewDimension::Cube),
                ..default()
            }),
            ..Image::new(
                Extent3d {
                    width: FACE,
                    height: FACE,
                    depth_or_array_layers: 6,
                },
                TextureDimension::D2,
                data,
                // Half floats, not 8-bit: a cloudless sky is one large smooth gradient, which is
                // the worst case for banding, and this is the one HDR format WebGL2 filters
                // without an extension.
                TextureFormat::Rgba16Float,
                RenderAssetUsages::RENDER_WORLD,
            )
        }
    }

    /// The sky as a `EnvironmentMapLight::hemispherical_gradient` takes it: zenith, horizon, ground.
    ///
    /// What lights the scene and what the water reflects therefore come from the same model as what
    /// is drawn behind them, rather than from three colours matched by eye that drift the moment the
    /// sun moves.
    ///
    /// The horizon is averaged around the compass because a gradient has one horizon colour and the
    /// real sky does not — it is far brighter towards the sun. The mean is the honest summary of a
    /// distinction this approximation cannot represent.
    pub fn gradient_colours(&self) -> (Color, Color, Color) {
        let model = Preetham::new(self.sun, self.turbidity);
        (
            linear(self.shade(&model, Vec3::Y)),
            linear(model.horizon),
            linear(self.ground_colour(&model)),
        )
    }

    /// Sky above, ground below, hazing between the two — the whole picture for one direction.
    fn shade(&self, model: &Preetham, dir: Vec3) -> Vec3 {
        let elevation = dir.y.clamp(-1.0, 1.0).asin();
        if elevation >= 0.0 {
            return model.radiance(dir);
        }

        // Below the horizon the sky is evaluated at the horizon in this same azimuth, so the two
        // sides of the blend agree exactly at the join and there is no seam to place.
        let flat = Vec3::new(dir.x, 0.0, dir.z).normalize_or(Vec3::X);
        let t = smoothstep((-elevation / self.haze).min(1.0));
        model.radiance(flat).lerp(self.ground_colour(model), t)
    }

    /// A neutral grey at [`Sky::ground`] of the sky's mean brightness at the horizon.
    ///
    /// Flat, rather than tracking the horizon azimuth by azimuth: the ground is one surface and
    /// reads as one, while the haze over it carries whatever variation the sky has.
    fn ground_colour(&self, model: &Preetham) -> Vec3 {
        Vec3::splat(luminance(model.horizon) * self.ground)
    }
}

/// Rec. 709 luminance, the weighting the eye gives linear RGB.
fn luminance(rgb: Vec3) -> f32 {
    rgb.dot(Vec3::new(0.2126, 0.7152, 0.0722))
}

fn linear(rgb: Vec3) -> Color {
    LinearRgba::rgb(rgb.x, rgb.y, rgb.z).into()
}

/// Hermite ease, so the haze has no visible edge where it starts or where it finishes.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// The five Perez coefficients for one channel.
///
/// `F(θ, γ) = (1 + a·e^(b / cos θ)) · (1 + c·e^(d·γ) + e·cos²γ)` — the first bracket grades the sky
/// from zenith to horizon, the second places the glow around the sun.
struct Perez {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
}

impl Perez {
    /// From the per-channel `(slope, intercept)` pairs Preetham fits against turbidity.
    fn new(coefficients: [(f32, f32); 5], turbidity: f32) -> Self {
        let at = |i: usize| coefficients[i].0 * turbidity + coefficients[i].1;
        Self {
            a: at(0),
            b: at(1),
            c: at(2),
            d: at(3),
            e: at(4),
        }
    }

    fn eval(&self, cos_theta: f32, gamma: f32) -> f32 {
        // e^(b / cos θ) runs away as a direction approaches the horizon, where the model stops
        // being defined at all. Clamping the cosine bounds it just short of that, which is also
        // where the ground has taken over the picture anyway.
        let cos_theta = cos_theta.max(0.01);
        (1.0 + self.a * (self.b / cos_theta).exp())
            * (1.0 + self.c * (self.d * gamma).exp() + self.e * gamma.cos().powi(2))
    }
}

/// Preetham's daylight model, with everything that depends only on the sun and the turbidity
/// resolved up front.
struct Preetham {
    luminance: Perez,
    chroma_x: Perez,
    chroma_y: Perez,
    /// Zenith luminance in cd/m², and the zenith chromaticity `(x, y)`.
    zenith: Vec3,
    /// `F(0, θs)` per channel: the normalisation that makes the model return `zenith` overhead.
    denominator: Vec3,
    sun: Vec3,
    /// The sky at the horizon, averaged around the compass.
    ///
    /// Averaged because the real horizon is far brighter towards the sun than away from it, and
    /// both the things this feeds — the ground's grey and the environment map's single horizon
    /// colour — are approximations that have one value to give. The mean is the honest summary of
    /// a distinction neither can represent.
    horizon: Vec3,
}

impl Preetham {
    fn new(sun: Vec3, turbidity: f32) -> Self {
        let sun = sun.normalize_or(Vec3::Y);
        let t = turbidity;

        let luminance = Perez::new(
            [
                (0.1787, -1.4630),
                (-0.3554, 0.4275),
                (-0.0227, 5.3251),
                (0.1206, -2.5771),
                (-0.0670, 0.3703),
            ],
            t,
        );
        let chroma_x = Perez::new(
            [
                (-0.0193, -0.2592),
                (-0.0665, 0.0008),
                (-0.0004, 0.2125),
                (-0.0641, -0.8989),
                (-0.0033, 0.0452),
            ],
            t,
        );
        let chroma_y = Perez::new(
            [
                (-0.0167, -0.2608),
                (-0.0950, 0.0092),
                (-0.0079, 0.2102),
                (-0.0441, -1.6537),
                (-0.0109, 0.0529),
            ],
            t,
        );

        // Sun zenith angle. Everything below is fitted in terms of it.
        let theta_s = sun.y.clamp(-1.0, 1.0).acos();
        let chi = (4.0 / 9.0 - t / 120.0) * (std::f32::consts::PI - 2.0 * theta_s);
        // Preetham gives this in kcd/m².
        let y_z = ((4.0453 * t - 4.9710) * chi.tan() - 0.2155 * t + 2.4192) * 1000.0;

        let (s, s2, s3) = (theta_s, theta_s * theta_s, theta_s * theta_s * theta_s);
        let x_z = t * t * (0.00166 * s3 - 0.00375 * s2 + 0.00209 * s)
            + t * (-0.02903 * s3 + 0.06377 * s2 - 0.03202 * s + 0.00394)
            + (0.11693 * s3 - 0.21196 * s2 + 0.06052 * s + 0.25885);
        let y_c = t * t * (0.00275 * s3 - 0.00610 * s2 + 0.00317 * s)
            + t * (-0.04214 * s3 + 0.08970 * s2 - 0.04153 * s + 0.00515)
            + (0.15346 * s3 - 0.26756 * s2 + 0.06669 * s + 0.26688);

        // Straight up is θ = 0, and the sun sits γ = θs away from it.
        let denominator = Vec3::new(
            luminance.eval(1.0, theta_s),
            chroma_x.eval(1.0, theta_s),
            chroma_y.eval(1.0, theta_s),
        );

        let mut model = Self {
            luminance,
            chroma_x,
            chroma_y,
            zenith: Vec3::new(y_z.max(0.0), x_z, y_c),
            denominator,
            sun,
            horizon: Vec3::ZERO,
        };

        const AZIMUTHS: usize = 8;
        for i in 0..AZIMUTHS {
            let angle = std::f32::consts::TAU * i as f32 / AZIMUTHS as f32;
            model.horizon += model.radiance(Vec3::new(angle.cos(), 0.0, angle.sin()));
        }
        model.horizon /= AZIMUTHS as f32;
        model
    }

    /// Linear RGB radiance along a **normalized** `dir`, in cd/m².
    fn radiance(&self, dir: Vec3) -> Vec3 {
        let cos_theta = dir.y;
        let gamma = dir.dot(self.sun).clamp(-1.0, 1.0).acos();

        let f = Vec3::new(
            self.luminance.eval(cos_theta, gamma),
            self.chroma_x.eval(cos_theta, gamma),
            self.chroma_y.eval(cos_theta, gamma),
        );
        let xyy = self.zenith * f / self.denominator;
        xyy_to_linear_rgb(xyy.y, xyy.z, xyy.x.max(0.0))
    }
}

/// CIE xyY to linear sRGB, via XYZ.
///
/// Negatives are clipped rather than gamut-mapped: they only appear for chromaticities outside
/// sRGB, which for a daylight sky means a sliver near the horizon that the ground is hazing over
/// regardless.
fn xyy_to_linear_rgb(x: f32, y: f32, big_y: f32) -> Vec3 {
    if y <= 1e-5 {
        return Vec3::ZERO;
    }
    let big_x = x * big_y / y;
    let big_z = (1.0 - x - y) * big_y / y;

    Vec3::new(
        3.2406 * big_x - 1.5372 * big_y - 0.4986 * big_z,
        -0.9689 * big_x + 1.8758 * big_y + 0.0415 * big_z,
        0.0557 * big_x - 0.2040 * big_y + 1.0570 * big_z,
    )
    .max(Vec3::ZERO)
}

/// `f32` to IEEE half-precision bits.
///
/// Hand-rolled because the `half` crate is not a dependency and is not going to become one for
/// fifteen lines, and Rust's own `f16` is still unstable. Truncating rather than rounding to
/// nearest, which costs a fraction of a bit on a texture that is already an approximation of the
/// sky; overflow saturates to the largest finite half rather than becoming an infinity, since an
/// infinity in a skybox is a white hole rather than a bright patch.
fn f16_bits(x: f32) -> u16 {
    let bits = x.to_bits();
    let sign = ((bits >> 16) & 0x8000) as u16;
    let exponent = ((bits >> 23) & 0xff) as i32 - 127;
    let mantissa = bits & 0x007f_ffff;

    if exponent > 15 {
        return sign | 0x7bff;
    }
    if exponent < -24 {
        return sign;
    }
    if exponent < -14 {
        // Subnormal: the implicit leading bit becomes explicit and the whole thing shifts down.
        let shift = (-14 - exponent) as u32;
        return sign | ((mantissa | 0x0080_0000) >> (shift + 13)) as u16;
    }
    sign | (((exponent + 15) as u16) << 10) | (mantissa >> 13) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::luminance as luma;

    #[test]
    fn half_precision_matches_the_known_bit_patterns() {
        for (value, bits) in [
            (0.0, 0x0000),
            (1.0, 0x3c00),
            (-1.0, 0xbc00),
            (0.5, 0x3800),
            (2.0, 0x4000),
            (65504.0, 0x7bff),
            (1.0e30, 0x7bff),  // saturates, no infinity
            (1.0e-30, 0x0000), // underflows to zero
            (6.0e-8, 0x0001),  // smallest subnormal
        ] {
            assert_eq!(f16_bits(value), bits, "{value} should be {bits:#06x}");
        }
    }

    /// The face table is the one thing here that can be silently wrong — a sign flip or a swapped
    /// pair renders a sky that looks plausible until you notice the ground is overhead. Sampling
    /// the poles catches every one of them.
    #[test]
    fn the_faces_point_where_they_claim() {
        let centre = |face: usize| FACES[face](0.0, 0.0).normalize();
        for (face, expected) in [
            (0, Vec3::X),
            (1, Vec3::NEG_X),
            (2, Vec3::Y),
            (3, Vec3::NEG_Y),
            (4, Vec3::Z),
            (5, Vec3::NEG_Z),
        ] {
            assert!(
                centre(face).abs_diff_eq(expected, 1e-6),
                "face {face} centre is {:?}, expected {expected:?}",
                centre(face)
            );
        }
    }

    #[test]
    fn the_sun_is_the_brightest_part_of_the_sky() {
        let sky = Sky::default();
        let towards_sun = luma(sky.radiance(sky.sun));
        assert!(
            towards_sun > luma(sky.radiance(Vec3::Y)),
            "brighter than the zenith"
        );
        assert!(
            towards_sun > luma(sky.radiance(-Vec3::new(sky.sun.x, -sky.sun.y, sky.sun.z))),
            "brighter than the anti-solar sky"
        );
    }

    /// A clear sky is blue overhead and pales towards the horizon. Both halves matter: the paling
    /// is what the water reflects at grazing angles, and if the ratio inverted the sky would read
    /// as an overcast dome.
    #[test]
    fn the_zenith_is_bluer_and_darker_than_the_horizon() {
        let sky = Sky::default();
        let zenith = sky.radiance(Vec3::Y);
        // Away from the sun, so this is the sky grading rather than the solar halo.
        let horizon = sky.radiance(Vec3::new(-sky.sun.x, 0.02, -sky.sun.z));

        assert!(luma(zenith) < luma(horizon), "the horizon is the brighter");
        assert!(
            zenith.z / zenith.x > horizon.z / horizon.x,
            "the zenith is the bluer: {zenith:?} vs {horizon:?}"
        );
    }

    /// The horizon is a haze, not a line. Asserting the property rather than the picture: ground
    /// well below, sky well above, and a transition between them that only ever goes one way. A
    /// hard edge, an inverted blend, or a band that overshoots all fail this.
    #[test]
    fn ground_hazes_into_sky_without_a_step() {
        let sky = Sky::default();
        // Across the compass, since the haze blends towards a horizon colour that varies with
        // azimuth and a mistake there would show on one side only.
        for i in 0..8 {
            let angle = std::f32::consts::TAU * i as f32 / 8.0;
            let at = |elevation: f32| {
                let (s, c) = elevation.sin_cos();
                sky.radiance(Vec3::new(angle.cos() * c, s, angle.sin() * c))
            };

            let bare = sky.ground_colour(&Preetham::new(sky.sun, sky.turbidity));
            assert!(
                at(-4.0 * sky.haze).abs_diff_eq(bare, 1e-3),
                "well below the horizon is bare ground"
            );

            // The ground is dark, but it is not a black void. Worth asserting because a downward
            // view fills the whole frame with it, so getting this wrong looks exactly like a
            // skybox that failed to load rather than like a tuning problem.
            let against_sky = luma(bare) / luma(at(0.0));
            assert!(
                (0.01..0.5).contains(&against_sky),
                "the ground is {against_sky} of the horizon: black void, or brighter than the sky"
            );

            // Samples from the bottom of the band up to the horizon, which is where the ground
            // gives way entirely.
            let steps = 32;
            let mut previous = f32::NEG_INFINITY;
            let mut biggest_jump = 0.0_f32;
            for step in 0..=steps {
                let elevation = -sky.haze * (1.0 - step as f32 / steps as f32);
                let level = luma(at(elevation));
                assert!(
                    level >= previous - 1e-7,
                    "the haze reverses at {elevation} rad, azimuth {angle}"
                );
                if previous.is_finite() {
                    biggest_jump = biggest_jump.max(level - previous);
                }
                previous = level;
            }

            // No single step may carry a large share of the whole climb, which is what an edge
            // hidden inside the band would look like.
            let climb = previous - luma(at(-sky.haze));
            assert!(
                biggest_jump < 0.25 * climb,
                "the haze has an edge in it: one step of {biggest_jump} out of {climb}"
            );
        }
    }

    #[test]
    fn the_cubemap_is_six_square_faces_of_half_floats() {
        let image = Sky::default().cubemap();
        assert_eq!(image.width(), FACE);
        assert_eq!(image.height(), FACE);
        assert_eq!(image.texture_descriptor.array_layer_count(), 6);
        assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba16Float);
        assert_eq!(
            image.texture_view_descriptor.as_ref().unwrap().dimension,
            Some(TextureViewDimension::Cube),
            "a skybox needs the cube view, or it renders as a stack of slices"
        );
        assert_eq!(
            image.data.as_ref().map(|d| d.len()),
            Some((FACE * FACE * 6 * 4 * 2) as usize),
            "four half-float channels per texel"
        );
    }
}
