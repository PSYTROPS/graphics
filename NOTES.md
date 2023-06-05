## Development notes
### Vulkan
* When creating Vulkan structures, use the `builder()` interface
    whenever possible (even for simple structures), or the `default()` object.
    These interfaces guarantee some invariants
    (such as `field_count` matching the size of a `field*` array).
    Know that the default values are usually zeroed bytes,
    which may not be valid values for some fields.
* Do not implement the `Drop` trait for structs that manage Vulkan objects.
    Instead, define a `destroy()` function that must be called manually.
    Vulkan object lifetimes do not map well to Rust lifetimes.
* Sometimes a function takes a slice of items as a parameter, but you only have one item.
    Use `std::slice::from_ref(&item)` instead of `&[item]` to avoid an unnecessary copy.
* Don't bother about error handling for now.
    It sounds nice to have proper error handling
    but the sheer volume of handling needed to cover every single Vulkan call
    isn't worth the effort at this stage,
    especially when most Vulkan errors are catastrophic anyways.
    Simply write functions with `Result<>` signatures
    in anticipation of future error handling implementation.
### Linear Algebra
* I have already tried the `nalgebra_glm` crate.
    It is missing the feature of being able to perform basic vector operations
    like addition & multiplication.
* The Vulkan coordinate system differs from OpenGL,
    so be careful when using graphics library routines.
