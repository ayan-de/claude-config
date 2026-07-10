import type { Metadata } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import Script from "next/script";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ThemeToggle } from "@/components/ThemeToggle";
import "./globals.css";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

const geistMono = Geist_Mono({
  variable: "--font-geist-mono",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Claude Config",
  description: "Manage Claude Code provider profiles",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${geistSans.variable} ${geistMono.variable} h-full antialiased`}
      suppressHydrationWarning
    >
      <head>
        <Script
          id="theme-bootstrap"
          strategy="beforeInteractive"
          dangerouslySetInnerHTML={{
            __html: `try { const t = localStorage.getItem('theme'); document.documentElement.classList.toggle('dark', t !== 'light'); } catch (_) {}`,
          }}
        />
      </head>
      <body className="h-full flex flex-col bg-background text-foreground overflow-hidden">
        <TooltipProvider>{children}</TooltipProvider>
        <ThemeToggle />
        <Toaster position="bottom-right" theme="dark" />
      </body>
    </html>
  );
}