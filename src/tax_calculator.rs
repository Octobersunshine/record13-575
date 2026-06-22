use serde::{Deserialize, Serialize};

use crate::hs_database::{ClassificationResult, HsCategory, MatchCandidate, TaxRisk};

#[derive(Debug, Deserialize)]
pub struct TaxCalculateRequest {
    pub hs_code: String,
    pub transaction_price: f64,
    pub freight: f64,
    pub insurance: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TaxCalculateResponse {
    pub hs_code: String,
    pub category: String,
    pub subcategory: String,
    pub description: String,
    pub cif_price: f64,
    pub transaction_price: f64,
    pub freight: f64,
    pub insurance: f64,
    pub tariff_rate: f64,
    pub tariff_amount: f64,
    pub vat_rate: f64,
    pub vat_amount: f64,
    pub consumption_tax_rate: f64,
    pub consumption_tax_amount: f64,
    pub comprehensive_tax_amount: f64,
    pub comprehensive_tax_rate: f64,
    pub total_price_after_tax: f64,
    pub match_level: String,
    pub match_score: f64,
    pub is_ambiguous: bool,
    pub normalized_hs_code: String,
    pub alternatives: Vec<MatchCandidate>,
    pub risks: Vec<TaxRisk>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

pub struct TaxCalculator;

impl TaxCalculator {
    pub fn calculate(req: &TaxCalculateRequest) -> Result<TaxCalculateResponse, String> {
        if req.transaction_price <= 0.0 {
            return Err("Transaction price must be greater than 0".to_string());
        }
        if req.freight < 0.0 {
            return Err("Freight cannot be negative".to_string());
        }

        let classification: ClassificationResult = crate::hs_database::HsDatabase::classify(&req.hs_code)
            .ok_or_else(|| format!("Unrecognized HS code: {}", req.hs_code))?;

        let category = &classification.primary.category;

        let insurance = req.insurance.unwrap_or_else(|| {
            (req.transaction_price + req.freight) * 0.003
        });

        if insurance < 0.0 {
            return Err("Insurance cannot be negative".to_string());
        }

        let cif_price = Self::round(req.transaction_price + req.freight + insurance);

        let tariff_amount = Self::round(cif_price * category.tariff_rate);

        let consumption_tax_amount = if category.consumption_tax_rate > 0.0 {
            let taxable_price = (cif_price + tariff_amount) / (1.0 - category.consumption_tax_rate);
            Self::round(taxable_price * category.consumption_tax_rate)
        } else {
            0.0
        };

        let vat_taxable = cif_price + tariff_amount + consumption_tax_amount;
        let vat_amount = Self::round(vat_taxable * category.vat_rate);

        let comprehensive_tax_amount =
            Self::round(tariff_amount + vat_amount + consumption_tax_amount);

        let comprehensive_tax_rate = if cif_price > 0.0 {
            Self::round(comprehensive_tax_amount / cif_price)
        } else {
            0.0
        };

        let total_price_after_tax = Self::round(cif_price + comprehensive_tax_amount);

        Ok(TaxCalculateResponse {
            hs_code: category.hs_code.clone(),
            category: category.category.clone(),
            subcategory: category.subcategory.clone(),
            description: category.description.clone(),
            cif_price,
            transaction_price: req.transaction_price,
            freight: req.freight,
            insurance,
            tariff_rate: category.tariff_rate,
            tariff_amount,
            vat_rate: category.vat_rate,
            vat_amount,
            consumption_tax_rate: category.consumption_tax_rate,
            consumption_tax_amount,
            comprehensive_tax_amount,
            comprehensive_tax_rate,
            total_price_after_tax,
            match_level: classification.primary.match_level.clone(),
            match_score: classification.primary.match_score,
            is_ambiguous: classification.is_ambiguous,
            normalized_hs_code: classification.normalized_hs_code.clone(),
            alternatives: classification.alternatives,
            risks: classification.risks,
        })
    }

    pub fn list_categories() -> Vec<HsCategory> {
        crate::hs_database::HsDatabase::list_categories()
    }

    fn round(value: f64) -> f64 {
        (value * 100.0).round() / 100.0
    }
}
