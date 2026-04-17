/** @type {import('next').NextConfig} */
const BACKEND =
  process.env.BAITER_BACKEND_URL ?? "http://127.0.0.1:3000";

const nextConfig = {
  async rewrites() {
    return [
      { source: "/api/:path*", destination: `${BACKEND}/api/:path*` },
    ];
  },
  async headers() {
    return [
      {
        source: "/api/events",
        headers: [{ key: "Cache-Control", value: "no-store" }],
      },
    ];
  },
};

export default nextConfig;
