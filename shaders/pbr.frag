#version 460
#define PI 3.14159

//Input
layout(location=0) in vec3 in_pos;
layout(location=1) in vec3 in_normal;
layout(location=2) in vec2 in_texcoords;
layout(location=3) in flat uint in_material;

//Output
layout(location=0) out vec4 out_color;

//Push constants
layout(push_constant) uniform constants {
	mat4 view;
};

//Descriptors
struct Material {
	vec4 color;
	uint color_tex;
	uint metal_rough_tex;
	float metal;
	float rough;
};
layout(std140, set=0, binding=1) restrict readonly buffer material_buffer {
	Material materials[];
};
layout(set=0, binding=2) uniform sampler s;
layout(set=0, binding=3) uniform texture2D textures[64];
struct PointLight {
	vec4 pos;
	vec4 color;
	float intensity;
	float range;
};
layout(std140, set=0, binding=4) restrict readonly buffer light_buffer {
	PointLight point_lights[64];
};
layout(set=0, binding=5) uniform samplerCube cubes[2];

//Distribution term
float distribution(vec3 l, vec3 v, vec3 n, float roughness) {
	const vec3 h = normalize(l + v);
	const float alpha = pow(roughness, 2);
	const float numer = pow(alpha, 2);
	const float denom = PI * pow(pow(max(dot(n, h), 0), 2) * (pow(alpha, 2) - 1) + 1, 2);
	return numer / denom;
}

//Geometry term
float geometry(vec3 l, vec3 v, vec3 n, float roughness) {
	const float k = pow(roughness + 1, 2) / 8.0;
	const float nv = max(dot(n, v), 0);
	const float g1 = nv / (nv * (1 - k) + k);
	const float nl = max(dot(n, l), 0);
	const float g2 = nl / (nl * (1 - k) + k);
	return g1 * g2;
}

//Fresnel term
vec3 fresnel(vec3 l, vec3 n, vec3 albedo, float metalness) {
	const vec3 f0 = mix(vec3(0.04), albedo, metalness);
	return f0 + (1 - f0) * pow(1 - max(dot(n, l), 0), 5);
}

void main() {
	//Material
	const Material material = materials[in_material];
	const vec4 albedo = material.color * texture(
		sampler2D(textures[material.color_tex], s),
		in_texcoords
	);
	const vec4 metal_rough_map = texture(
		sampler2D(textures[material.metal_rough_tex], s),
		in_texcoords
	);
	//Reflectance equation
	const vec3 v = -vec3(in_pos);
	const vec3 n = in_normal;
	vec3 outgoing = vec3(0.002) * vec3(albedo);
	for (uint i = 0; i < 64; ++i) {
		//Light
		const PointLight light = point_lights[i];
		const vec3 light_pos = vec3(view * light.pos);
		const vec3 l = normalize(light_pos - in_pos);
		const float light_dist = distance(light_pos, in_pos);
		const float attenuation = max(min(1 - pow(light_dist / light.range, 4), 1), 0) / pow(light_dist, 2);
		const vec3 radiance = attenuation * light.intensity * vec3(light.color);
		//Material
		const float metallic = material.metal * metal_rough_map.b;
		const float roughness = material.rough * metal_rough_map.g;
		//Specular
		const float d = distribution(l, v, n, roughness);
		const float g = geometry(l, v, n, roughness);
		const vec3 f = fresnel(l, n, vec3(albedo), metallic);
		const vec3 specular = d * g * f / (4 * max(dot(n, l), 0) * max(dot(n, v), 0) + 0.0001);
		//Diffuse
		const vec3 diffuse = vec3(albedo) / PI;
		//BDRF
		const vec3 reflectance = specular + (1 - metallic) * (1 - f) * diffuse;
		outgoing += reflectance * radiance * dot(n, l);
	}
	out_color = vec4(outgoing, 1.0);
}
