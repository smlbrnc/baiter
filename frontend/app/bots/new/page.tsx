import { BotForm } from "@/components/bots/bot-form";

export default function NewBotPage() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-xl font-semibold tracking-tight">Yeni Bot</h1>
        <p className="text-muted-foreground text-xs">
          Slug kalıbı, strateji ve mod seç
        </p>
      </div>
      <BotForm />
    </div>
  );
}
