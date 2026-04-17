import { BotForm } from "@/components/BotForm";

export function NewBot() {
  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-semibold tracking-tight">Yeni Bot</h1>
      <BotForm />
    </div>
  );
}
