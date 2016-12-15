# rkt stage1 glide

> It's a bird; it's a plane... no it's a new stage1 that can barely get off the ground

## Introduction

Hello folks, in the spirit of the existing rkt stage1's, I thought it would be
fun to see exactly how far we could stretch the definition!

First of all, the name. rkt in general is a tool for launching a bundle of binaries into space (a glorious goal that is) and making sure they don't crash into your pretty planet from whence you launched them.

However, there's also stage1 fly which trades the rkt engines for just having jet engines and only launches your wonderful bundle of bytes as far as the upper atmosphere. Sure, it's pretty far from doing serious damage to the land below, but it wouldn't be hard for a malicious pilot to point that nose down.

In continuing the trend, I wanted to make stage1 glide. It's like stage1 fly, except you know that inevitably it'll crash into the ground and hurt everything. It doesn't take a malicious man to hurt some stuff with a glider and, well, you shouldn't really trust it with anything important.

However, gliders are a hell of a lot more fun and affordable than planes or rkts -- they're not totally useless.

The gist I'm trying to get at is you can use stage1 glide if you want something that's super cheap and fun, and quite possibly a bad idea!

## Uhhh... what?

Stage1 glide runs an application under the rkt container engine with no isolation. It modifies only the working directory, path, and library path of the provided image. It is a bad idea. It should not be trusted. It really should not be used. For anything.

## Language?

Rust all the way! If you see a panic, nil pointer exception, or type error then it's probably rkt stage0 to blame (or you forgot to set the stage1 to this).

## Does anything actually even run in this? Seriously?

Things run better if you use relative paths!

# FAQ

## Bindmounts????????

No??????

## Networking???

Host!

## Privileged?

Please read the "Uhhh... what?" section again

## License?

Apache 2.0 to match rkt
