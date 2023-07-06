#version 460
#define PI 3.14159

//Input
layout(location=0) in vec3 in_pos;
layout(location=1) in vec3 in_normal;
layout(location=2) in vec2 in_texcoords;
layout(location=3) in flat uint in_material;

//Output
layout(location=0) out vec4 out_color;

//Descriptors
layout(set=0, binding=0) uniform camera {
	mat4 view;
	mat4 projection;
	vec4 camera_pos;
};
struct Material {
	vec4 color;
	uint color_tex;
	uint metal_rough_tex;
	float metal;
	float rough;
};
layout(std430, set=0, binding=2) restrict readonly buffer material_buffer {
	Material materials[];
};
layout(set=0, binding=5) uniform sampler s;
layout(set=0, binding=6) uniform texture2D textures[64];
struct PointLight {
	vec4 pos;
	vec4 color;
	float intensity;
	float range;
};
layout(std430, set=0, binding=7) restrict readonly buffer light_buffer {
	PointLight point_lights[64];
};
layout(set=0, binding=8) uniform samplerCube cubes[2];
layout(set=0, binding=9) uniform sampler2D dfgLUT;

// Remapped and clamped roughness
float alpha(float roughness) {
	return max(roughness * roughness, 0.001); // 0.001 seems to eliminate specular aliasing
}

//Distribution term
float distribution(float nh, float a) {
	const float a2 = a * a;
	const float div = nh * nh * (a2 - 1) + 1;
	return a2 / (PI * div * div);
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
vec3 fresnel(float vh, vec3 f0) {
	return f0 + (1 - f0) * pow(1 - vh, 5);
}

vec3 aces_tonemap(vec3 hdr) {
	vec3 aces = hdr * 0.6;
	float a = 2.51;
	float b = 0.03;
	float c = 2.43;
	float d = 0.59;
	float e = 0.14;
	return clamp((aces * (a * aces + b)) / (aces * (c * aces + d) + e), 0.0, 1.0);
}

void main() {
	//Material
	const Material material = materials[in_material];
	const vec3 albedo = vec3(material.color * texture(
		sampler2D(textures[material.color_tex], s),
		in_texcoords
	));
	const vec4 metal_rough_map = texture(
		sampler2D(textures[material.metal_rough_tex], s),
		in_texcoords
	);
	const float metallic = material.metal * metal_rough_map.b;
	const float roughness = material.rough * metal_rough_map.g;
	const float a = alpha(roughness);
	//Lighting vectors
	const vec3 cameraPos = camera_pos.xyz;
	const vec3 v = normalize(cameraPos - in_pos);
	const vec3 n = in_normal;
	const float nv = max(dot(n, v), 0);
	//Diffuse & specular
	const vec3 diffColor = (1 - metallic) * albedo;
	const vec3 f0 = mix(vec3(0.04), albedo, metallic);
	//IBL
	const vec2 dfg = textureLod(dfgLUT, vec2(nv, roughness), 0).xy;
	vec3 multiscatter = 1.0f + f0 * (1.0f / dfg.y - 1.0f);
	//Reflectance equation
	vec3 outgoing = vec3(0.0);
	for (uint i = 0; i < 64; ++i) {
		//Light
		const PointLight light = point_lights[i];
		const vec3 l = normalize(light.pos.xyz - in_pos);
		const vec3 h = normalize(v + l);
		const float nh = max(dot(n, h), 0);
		const float nl = max(dot(n, l), 0);
		const float vh = max(dot(v, h), 0);
		const float light_dist = distance(light.pos.xyz, in_pos);
		const float attenuation = max(min(1 - pow(light_dist / light.range, 4), 1), 0) / pow(light_dist, 2);
		const vec3 radiance = attenuation * light.intensity * vec3(light.color);
		//Specular
		const float d = distribution(nh, a);
		const float g = geometry(l, v, n, roughness);
		const vec3 f = fresnel(vh, f0);
		const vec3 specular = d * g * f / (4 * nl * nv + 0.0001);
		//Diffuse
		const vec3 diffuse = diffColor / PI;
		//BDRF
		const vec3 reflectance = multiscatter * specular + (1 - f) * diffuse;
		outgoing += reflectance * radiance * nl;
	}
	//IBL
	const vec3 f = fresnel(nv, f0);
	const vec3 ibl_diffuse = diffColor * textureLod(cubes[0], n, 0).xyz;
	const vec3 ibl_specular = textureLod(cubes[1], reflect(-v, n), roughness * 11).xyz * mix(dfg.xxx, dfg.yyy, f0);
	out_color = vec4(aces_tonemap(outgoing + ibl_specular + (1 - f) * ibl_diffuse), 1.0);
}
