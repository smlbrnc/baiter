import { Route, Routes } from "react-router-dom";
import { AppShell } from "@/components/AppShell";
import { Dashboard } from "@/pages/Dashboard";
import { NewBot } from "@/pages/NewBot";
import { BotDetail } from "@/pages/BotDetail";

export default function App() {
  return (
    <AppShell>
      <Routes>
        <Route path="/" element={<Dashboard />} />
        <Route path="/bots/new" element={<NewBot />} />
        <Route path="/bots/:id" element={<BotDetail />} />
      </Routes>
    </AppShell>
  );
}
