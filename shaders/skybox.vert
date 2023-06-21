#version 460

layout(location=0) in vec3 in_pos;
layout(location=0) out vec3 out_pos;

//Push constants
layout(push_constant) uniform constants {
	mat4 view;
	mat4 projection;
};

void main() {
	vec4 pos = view * vec4(in_pos, 0.0);
	pos.w = 1.0;
	pos = projection * pos;
	gl_Position = pos.xyww;
	out_pos = in_pos;
}