use ash::vk;
use super::arena::Arena;

pub struct BufferTransfer {
    pub src_offset: usize,
    pub size: usize,
    pub dst: vk::Buffer,
    pub dst_offset: usize
}

pub struct ImageTransfer {
    pub dst: vk::Image,
    pub subresource_range: vk::ImageSubresourceRange,
    pub region_offset: usize,
    pub region_count: usize,
    pub layout: vk::ImageLayout
}

pub struct Transaction {
    src_queue_family: u32,
    dst_queue_family: u32,
    pub arena: Arena,
    //Buffers
    pub buffer_transfers: Vec<BufferTransfer>,
    //buffer_barriers: Vec<vk::BufferMemoryBarrier2>,
    //Images
    pub image_transfers: Vec<ImageTransfer>,
    pub regions: Vec<vk::BufferImageCopy2>,
    pub start_image_barriers: Vec<vk::ImageMemoryBarrier2>,
    pub end_image_barriers: Vec<vk::ImageMemoryBarrier2>
}

impl Transaction {
    pub fn new(
        src_queue_family: u32,
        dst_queue_family: u32
    ) -> Self {
        Self {
            src_queue_family,
            dst_queue_family,
            arena: Arena::new(0),
            buffer_transfers: vec![],
            //buffer_barriers: vec![],
            image_transfers: vec![],
            regions: vec![],
            start_image_barriers: vec![],
            end_image_barriers: vec![]
        }
    }

    pub fn buffer_write<T>(
        &mut self,
        src: &[T],
        dst: vk::Buffer,
        dst_offset: usize
    ) {
        let src_offset = self.arena.extend(src);
        let size = std::mem::size_of::<T>() * src.len();
        self.buffer_transfers.push(BufferTransfer {
            src_offset,
            size,
            dst,
            dst_offset
        });
    }

    pub fn image_write<T>(
        &mut self,
        src: &[T],
        dst: vk::Image,
        subresource_range: vk::ImageSubresourceRange,
        regions: &[vk::BufferImageCopy2],
        layout: vk::ImageLayout
    ) {
        let src_offset = self.arena.extend(src);
        let region_offset = self.regions.len();
        for region in regions {
            let mut new_region = region.clone();
            new_region.buffer_offset += src_offset as u64;
            self.regions.push(new_region);
        }
        self.image_transfers.push(ImageTransfer {
            dst,
            subresource_range,
            region_offset,
            region_count: regions.len(),
            layout
        });
        self.start_image_barriers.push(*vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(self.src_queue_family)
            .dst_queue_family_index(self.src_queue_family)
            .image(dst)
            .subresource_range(subresource_range)
        );
        self.end_image_barriers.push(*vk::ImageMemoryBarrier2::builder()
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(layout)
            .src_queue_family_index(self.src_queue_family)
            .dst_queue_family_index(self.dst_queue_family)
            .image(dst)
            .subresource_range(subresource_range)
        );
    }

    pub fn clear(&mut self) {
        self.arena.clear();
        self.buffer_transfers.clear();
        //self.buffer_barriers.clear();
        self.image_transfers.clear();
        self.start_image_barriers.clear();
        self.end_image_barriers.clear();
        self.regions.clear();
    }
}
