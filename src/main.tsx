#!/usr/bin/env node
import { render } from "ink";
import { App } from "./app.js";

// The current working directory is the project root (Stage 1 rule).
render(<App root={process.cwd()} />);
