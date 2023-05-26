#version 460

layout(location=0) in vec3 in_pos;
layout(location=1) in vec3 in_color;
layout(location=0) out vec4 out_color;

void main() {
	const vec4 pos = vec4(in_pos, 1.0);
	const vec4 color = vec4(in_color, 1.0);
	gl_Position = pos;
	out_color = color;
}
