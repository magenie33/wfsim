// SPDX-License-Identifier: AGPL-3.0-or-later

/// Client coordinates: Y is up. Unit conversion belongs to the supplied scene.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vec3(pub [f64; 3]);

#[wfsim_gas_macro::metered]
impl Vec3 {
    pub fn dot(self, b: Self) -> f64 {
        (self.0[0] * b.0[0] + self.0[1] * b.0[1]) + self.0[2] * b.0[2]
    }

    pub fn minus(self, b: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] - b.0[i]))
    }

    pub fn along(self, direction: Self, distance: f64) -> Self {
        Self(std::array::from_fn(|i| {
            self.0[i] + direction.0[i] * distance
        }))
    }

    pub fn length(self) -> f64 {
        libm::sqrt(self.dot(self))
    }

    pub fn normalized(self) -> Option<Self> {
        let length = self.length();
        (length > 0.0 && length.is_finite()).then(|| Self(self.0.map(|v| v / length)))
    }
}

/// Row-major 3x4 affine transform, applied to a column vector.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Affine(pub [f64; 12]);

#[wfsim_gas_macro::metered]
impl Affine {
    pub const IDENTITY: Self = Self([1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0.]);

    pub fn point(self, p: Vec3) -> Vec3 {
        let mut out = self.direction(p);
        for i in 0..3 {
            out.0[i] += self.0[i * 4 + 3];
        }
        out
    }

    pub fn direction(self, p: Vec3) -> Vec3 {
        Vec3(std::array::from_fn(|i| {
            let row = &self.0[i * 4..];
            (row[0] * p.0[0] + row[1] * p.0[1]) + row[2] * p.0[2]
        }))
    }

    pub fn compose(self, local: Self) -> Self {
        Self(std::array::from_fn(|i| {
            let (r, c) = (i / 4, i % 4);
            let product = (self.0[r * 4] * local.0[c] + self.0[r * 4 + 1] * local.0[4 + c])
                + self.0[r * 4 + 2] * local.0[8 + c];
            product + if c == 3 { self.0[r * 4 + 3] } else { 0.0 }
        }))
    }

    pub fn inverse(self) -> Option<Self> {
        let [a, b, c, p, d, e, f, q, g, h, i, s] = self.0;
        let determinant = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        let mut out = Self([
            (e * i - f * h) / determinant,
            (c * h - b * i) / determinant,
            (b * f - c * e) / determinant,
            0.,
            (f * g - d * i) / determinant,
            (a * i - c * g) / determinant,
            (c * d - a * f) / determinant,
            0.,
            (d * h - e * g) / determinant,
            (b * g - a * h) / determinant,
            (a * e - b * d) / determinant,
            0.,
        ]);
        let translation = out.direction(Vec3([p, q, s]));
        for row in 0..3 {
            out.0[row * 4 + 3] = -translation.0[row];
        }
        Some(out)
    }
}
