// SPDX-License-Identifier: MIT
// Copyright (c) 2026 xqy2006. See LICENSE and README.md in this directory.
export interface Challenge { id:string; expected_count:number; prompt:string }
export interface BankModel {id:string;display_name:string;family:string;family_name:string;response_count:number;valid_number_count:number;counts:number[];frequency_references?:number[][];sources:Record<string,number>;conditions:Record<string,number>;nearest_models?:{model:string;distance:number}[]}
export interface Bank {schema:string;built_at:string;models:BankModel[];sources:Record<string,number>;reference_sha256?:string;robust:any;calibration:Record<string,{beta:number;cv_accuracy:number|null;[key:string]:unknown}>;[key:string]:unknown}
export interface Analysis {prediction:string;prediction_name:string;probability:number|null;evidence:{insufficient:boolean;label:string;reason:string;threshold:number|null;method:string};used_outputs:number;results:{model:string;display_name:string;family:string;family_name:string;probability:number|null;score:number}[];diagnostics:{index:number;parsed_numbers:number;minimum_numbers:number;accepted:boolean}[];family_prediction_name:string;ranking_score?:number;decision?:string;sequence?:{autocorrelation:number;small_step_share:number;model_like:boolean}|null;[key:string]:unknown}
export interface Output {text:string;expected_count:number}
