/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use font_context::FontContext;
use style::computed_values::border_style;
use opts::Opts;

use azure::azure_hl::{B8G8R8A8, Color, ColorPattern, DrawOptions};
use azure::azure_hl::{DrawSurfaceOptions, DrawTarget, Linear, StrokeOptions};
use azure::{AZ_CAP_BUTT, AZ_CAP_ROUND};
use azure::AZ_JOIN_BEVEL;
use azure::AzFloat;
use extra::arc::Arc;
use geom::point::Point2D;
use geom::rect::Rect;
use geom::size::Size2D;
use geom::side_offsets::SideOffsets2D;
use servo_net::image::base::Image;
use servo_util::geometry::Au;
use std::vec;
use std::libc::types::common::c99::uint16_t;
use std::libc::size_t;

pub struct RenderContext<'self> {
    draw_target: &'self DrawTarget,
    font_ctx: @mut FontContext,
    opts: &'self Opts,
    /// The rectangle that this context encompasses in page coordinates.
    page_rect: Rect<f32>,
    /// The rectangle that this context encompasses in screen coordinates (pixels).
    screen_rect: Rect<uint>,
}


enum Direction {
        Top,
        Left,
        Right,
        Bottom
}

impl<'self> RenderContext<'self>  {

    pub fn get_draw_target(&self) -> &'self DrawTarget {
        self.draw_target
    }

    pub fn draw_solid_color(&self, bounds: &Rect<Au>, color: Color) {
        self.draw_target.make_current();
        self.draw_target.fill_rect(&bounds.to_azure_rect(), &ColorPattern(color));
    }

    pub fn draw_border(&self,
                       bounds: &Rect<Au>,
                       border: SideOffsets2D<Au>,
                       color: SideOffsets2D<Color>,
                       style: SideOffsets2D<border_style::T>) {
        let border = border.to_float_px();

        self.draw_target.make_current();
 
	self.draw_border_segment(Top, bounds, border, color, style);
	self.draw_border_segment(Right, bounds, border, color, style);
	self.draw_border_segment(Bottom, bounds, border, color, style);
	self.draw_border_segment(Left, bounds, border, color, style);	
    }

    pub fn draw_image(&self, bounds: Rect<Au>, image: Arc<~Image>) {
        let image = image.get();
        let size = Size2D(image.width as i32, image.height as i32);
        let stride = image.width * 4;

        self.draw_target.make_current();
        let draw_target_ref = &self.draw_target;
        let azure_surface = draw_target_ref.create_source_surface_from_data(image.data, size,
                                                                            stride as i32, B8G8R8A8);
        let source_rect = Rect(Point2D(0 as AzFloat, 0 as AzFloat),
                               Size2D(image.width as AzFloat, image.height as AzFloat));
        let dest_rect = bounds.to_azure_rect();
        let draw_surface_options = DrawSurfaceOptions(Linear, true);
        let draw_options = DrawOptions(1.0f64 as AzFloat, 0);
        draw_target_ref.draw_surface(azure_surface,
                                     dest_rect,
                                     source_rect,
                                     draw_surface_options,
                                     draw_options);
    }

    pub fn clear(&self) {
        let pattern = ColorPattern(Color(1.0, 1.0, 1.0, 1.0));
        let rect = Rect(Point2D(self.page_rect.origin.x as AzFloat,
                                self.page_rect.origin.y as AzFloat),
                        Size2D(self.screen_rect.size.width as AzFloat,
                               self.screen_rect.size.height as AzFloat));
        self.draw_target.make_current();
        self.draw_target.fill_rect(&rect, &pattern);
    }

    fn draw_border_segment(&self, direction: Direction, bounds: &Rect<Au>, border: SideOffsets2D<f32>, color: SideOffsets2D<Color>, style: SideOffsets2D<border_style::T>) {
	let mut style_select = style.top;
	let mut color_select = color.top;

	match direction {
            Top => { style_select = style.top; color_select = color.top; }
            Left => { style_select = style.left; color_select = color.left; }
            Right => { style_select = style.right; color_select = color.right; }
            Bottom => { style_select = style.bottom; color_select = color.bottom; }
        }

        match style_select{
            border_style::none => {
            }
            border_style::hidden => {
            }
            //FIXME(sammykim): This doesn't work with dash_pattern and cap_style well. I referred firefox code.
            border_style::dotted => {
            }
            border_style::dashed => {
		self.draw_dashed_border_segment(direction,bounds,border,color_select);
            }
            border_style::solid => {
		self.draw_solid_border_segment(direction,bounds,border,color_select);
            }
            //FIXME(sammykim): Five more styles should be implemented.
            //double, groove, ridge, inset, outset
        }

	
    }

    fn draw_dashed_border_segment(&self, direction: Direction, bounds: &Rect<Au>, border: SideOffsets2D<f32>, color: Color) {
        let rect = bounds.to_azure_rect();
	let draw_opts = DrawOptions(1 as AzFloat, 0 as uint16_t);	
	let mut stroke_opts = StrokeOptions(0 as AzFloat, 10 as AzFloat);
	let mut dash: [AzFloat, ..2] = [0 as AzFloat, 0 as AzFloat];
	let mut start = Point2D(0.0 as f32,0.0 as f32);
	let mut end   = Point2D(0.0 as f32,0.0 as f32);

	stroke_opts.set_cap_style(AZ_CAP_BUTT as u8);
	
	fn test(){

	}

        match direction {
            Top => {
		let border_width = border.top;
	        stroke_opts.line_width = border_width;	
		dash[0] = border_width * 3 as AzFloat;
        	dash[1] = border_width * 3 as AzFloat;
        	stroke_opts.mDashPattern = vec::raw::to_ptr(dash);
        	stroke_opts.mDashLength = dash.len() as size_t;		
		let y = rect.origin.y + border.top * 0.5;
                start = Point2D(rect.origin.x, y);
                end = Point2D(rect.origin.x + rect.size.width, y);
            }
            Left => {
                let border_width = border.left;
	        stroke_opts.line_width = border_width;
		dash[0] = border_width * 3 as AzFloat;
       		dash[1] = border_width * 3 as AzFloat;
        	stroke_opts.mDashPattern = vec::raw::to_ptr(dash);
        	stroke_opts.mDashLength = dash.len() as size_t;
		let x = rect.origin.x + border.left * 0.5;
		start = Point2D(x, rect.origin.y + rect.size.height);	
                end = Point2D(x, rect.origin.y + border.top);
            }
            Right => {
                let border_width = border.right;
		stroke_opts.line_width = border_width;
		dash[0] = border_width * 3 as AzFloat;
        	dash[1] = border_width * 3 as AzFloat;
        	stroke_opts.mDashPattern = vec::raw::to_ptr(dash);
        	stroke_opts.mDashLength = dash.len() as size_t;
		let x = rect.origin.x + rect.size.width - border.right * 0.5;
        	start = Point2D(x, rect.origin.y);
        	end = Point2D(x, rect.origin.y + rect.size.height);
            }
            Bottom => {
                let border_width = border.bottom;
		stroke_opts.line_width = border_width;
		dash[0] = border_width * 3 as AzFloat;
        	dash[1] = border_width * 3 as AzFloat;
        	stroke_opts.mDashPattern = vec::raw::to_ptr(dash);
        	stroke_opts.mDashLength = dash.len() as size_t;
		let y = rect.origin.y + rect.size.height - border.bottom * 0.5;
                start = Point2D(rect.origin.x + rect.size.width, y);
        	end = Point2D(rect.origin.x + border.left, y);
            }
        }
		
	self.draw_target.stroke_line(start,
                                     end,
                                     &ColorPattern(color),
                                     &stroke_opts,
                                     &draw_opts);
    }

    fn draw_solid_border_segment(&self, direction: Direction, bounds: &Rect<Au>, border: SideOffsets2D<f32>, color: Color) {
        let rect = bounds.to_azure_rect();
	let draw_opts = DrawOptions(1 as AzFloat,0 as uint16_t);
        let path_builder = self.draw_target.create_path_builder();
	
	let left_top = Point2D(rect.origin.x, rect.origin.y);
	let right_top = Point2D(rect.origin.x + rect.size.width, rect.origin.y);
	let left_bottom = Point2D(rect.origin.x, rect.origin.y + rect.size.height);
	let right_bottom = Point2D(rect.origin.x + rect.size.width, rect.origin.y + rect.size.height); 

        match direction {
            Top => {
                path_builder.move_to(left_top);
                path_builder.line_to(right_top);
                path_builder.line_to(right_top + Point2D(-border.right, border.top));
                path_builder.line_to(left_top + Point2D(border.left, border.top));
            }
            Left => {
                path_builder.move_to(left_top);
                path_builder.line_to(left_top + Point2D(border.left, border.top));
		path_builder.line_to(left_bottom + Point2D(border.left, -border.bottom));
                path_builder.line_to(left_bottom);
            }
            Right => {
                path_builder.move_to(right_top);
                path_builder.line_to(right_bottom);
		path_builder.line_to(right_bottom + Point2D(-border.right, -border.bottom));
                path_builder.line_to(right_top + Point2D(-border.right, border.top));
            }
            Bottom => {
                path_builder.move_to(left_bottom);
                path_builder.line_to(left_bottom + Point2D(border.left, -border.bottom));
		path_builder.line_to(right_bottom + Point2D(-border.right, -border.bottom));
                path_builder.line_to(right_bottom);
            }
	}

        let path = path_builder.finish();
        self.draw_target.fill(&path, &ColorPattern(color), &draw_opts);	
    }

    fn apply_border_style(style: border_style::T, border_width: AzFloat, dash: &mut [AzFloat], stroke_opts: &mut StrokeOptions){
        match style{
            border_style::none => {
            }
            border_style::hidden => {
            }
            //FIXME(sammykim): This doesn't work with dash_pattern and cap_style well. I referred firefox code.
            border_style::dotted => {
                stroke_opts.line_width = border_width;
                
                if border_width > 2.0 {
                    dash[0] = 0 as AzFloat;
                    dash[1] = border_width * 2.0;

                    stroke_opts.set_cap_style(AZ_CAP_ROUND as u8);
                } else {
                    dash[0] = border_width;
                    dash[1] = border_width;
                }
                stroke_opts.mDashPattern = vec::raw::to_ptr(dash);
                stroke_opts.mDashLength = dash.len() as size_t;
            }
            border_style::dashed => {
                stroke_opts.set_cap_style(AZ_CAP_BUTT as u8);
                stroke_opts.line_width = border_width;
                dash[0] = border_width*3 as AzFloat;
                dash[1] = border_width*3 as AzFloat;
                stroke_opts.mDashPattern = vec::raw::to_ptr(dash);
                stroke_opts.mDashLength = dash.len() as size_t;
            }
            //FIXME(sammykim): BorderStyleSolid doesn't show proper join-style with comparing firefox.
            border_style::solid => {
                stroke_opts.set_cap_style(AZ_CAP_BUTT as u8);
                stroke_opts.set_join_style(AZ_JOIN_BEVEL as u8);
                stroke_opts.line_width = border_width; 
                stroke_opts.mDashLength = 0 as size_t;
            }            
            //FIXME(sammykim): Five more styles should be implemented.
            //double, groove, ridge, inset, outset
        }
    }
}

trait to_float {
    fn to_float(&self) -> f64;
}

impl to_float for u8 {
    fn to_float(&self) -> f64 {
        (*self as f64) / 255f64
    }
}

trait ToAzureRect {
    fn to_azure_rect(&self) -> Rect<AzFloat>;
}

impl ToAzureRect for Rect<Au> {
    fn to_azure_rect(&self) -> Rect<AzFloat> {
        Rect(Point2D(self.origin.x.to_nearest_px() as AzFloat,
                     self.origin.y.to_nearest_px() as AzFloat),
             Size2D(self.size.width.to_nearest_px() as AzFloat,
                    self.size.height.to_nearest_px() as AzFloat))
    }
}

trait ToSideOffsetsPx {
    fn to_float_px(&self) -> SideOffsets2D<AzFloat>;
}

impl ToSideOffsetsPx for SideOffsets2D<Au> {
    fn to_float_px(&self) -> SideOffsets2D<AzFloat> {
        SideOffsets2D::new(self.top.to_nearest_px() as AzFloat,
                           self.right.to_nearest_px() as AzFloat,
                           self.bottom.to_nearest_px() as AzFloat,
                           self.left.to_nearest_px() as AzFloat)
    }
}
