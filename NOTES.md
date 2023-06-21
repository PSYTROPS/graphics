## Development notes
### Vulkan
* When creating Vulkan structures, use the `builder()` interface
    whenever possible (even for simple structures), or the `default()` object.
    These interfaces guarantee some invariants
    (such as `field_count` matching the size of a `field*` array).
    Know that the default values are usually zeroed bytes,
    which may not be valid values for some fields.
* Sometimes a function takes a slice of items as a parameter, but you only have one item.
    Use `std::slice::from_ref(&item)` instead of `&[item]` to avoid an unnecessary copy.
* IMPORTANT: Ash builder objects which take a slice of items
    _will segfault_ when given a vector that goes out of scope
    before the builder object is consumed.
### Linear Algebra
* I have already tried the `nalgebra_glm` crate.
    It is missing the feature of being able to perform basic vector operations
    like addition & multiplication.
* The Vulkan coordinate system differs from OpenGL,
    so be careful when using graphics library routines.
    See `scene.rs` for details on our coordinate system.

## IBL
### Textures
1. Skybox textures
2. HDR environment map (2)
    * Irradiance map
    * Pre-filtered environment map
3. BRDF integration map