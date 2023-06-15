#version 460

//Input
layout(location=0) in vec3 in_pos;
layout(location=1) in vec3 in_normal;
layout(location=2) in vec2 in_texcoords;
layout(location=3) in flat uint in_material;

//Output
layout(location=0) out vec4 out_color;

//Descriptors
struct Material {
	vec4 color;
	uint color_tex;
	uint metal_rough_tex;
	float metal_factor;
	float rough_factor;
};
layout(std140, set=0, binding=1) restrict readonly buffer material_buffer {
	Material materials[];
};
layout(set=0, binding=2) uniform sampler s;
layout(set=0, binding=3) uniform texture2D textures[64];
struct PointLight {
	vec4 position;
	vec4 color;
	float intensity;
	float range;
};
layout(std140, set=0, binding=4) restrict readonly buffer light_buffer {
	PointLight point_lights[64];
};

void main() {
	const Material material = materials[in_material];
	const vec4 albedo = material.color * texture(
		sampler2D(textures[material.color_tex], s),
		in_texcoords
	);
	out_color = albedo;
}
