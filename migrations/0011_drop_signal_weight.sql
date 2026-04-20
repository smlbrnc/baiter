-- §14.4 refactor: signal_weight artık composite skoru hiçbir yerde gate'lemiyor.
-- OpenDual fiyatı + averaging size çarpanı doğrudan ham composite skoru kullanır.
-- HarvestContext::signal_multiplier yalnız MarketZone::HARVEST aktif mi diye bakar.
ALTER TABLE bots DROP COLUMN signal_weight;
