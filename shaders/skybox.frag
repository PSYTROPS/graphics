#version 460

layout(location=0) in vec3 in_pos;
layout(location=0) out vec4 out_color;
layout(set=0, binding=0) uniform samplerCube cube;

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
	vec3 color = textureLod(cube, in_pos, 0).xyz;
	color = aces_tonemap(color);
	out_color = vec4(color, 1.0);
}
