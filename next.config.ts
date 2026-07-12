import type { NextConfig } from "next";

const isDev = process.env.NODE_ENV !== "production";
const host = process.env.TAURI_DEV_HOST;

const nextConfig: NextConfig = {
  output: "export",
  images: { unoptimized: true },
  trailingSlash: true,
  assetPrefix: isDev && host ? `http://${host}:42713` : undefined,
  reactStrictMode: true,
};

export default nextConfig;